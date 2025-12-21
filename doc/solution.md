## タスクの挿入
1. `packet_type`ごとに`s=min_batch_size`のバッチを作成する
    - 処理
        - time_limitが近い順にpacketを選ぶ
        - そもそも間に合わないものは選択しない
2. コアごとに挿入できるタスクの総所要時間を計算する
    - `idle_task`があるなら挿入しない
        - TODO: `idle_task`についても調ベて、`min`をとる
    - `complete_t := cur_taskを完了するt`
    - `afford_duration := min_time_limit[cur_task] - complete_t`
    - TODO: タスクを他のコアに移動できるなら1回まで移動して、余裕を計算する
    - TODO: タスクをどこで分割すれば挿入できるか調べる
3. `timeout`する個数が多い順に`packet_type`、`afford_duration`が小さい順に挿入を試す
    - 条件
        - `new_task_duration < afford_duration`
        - `next_t[target_core] + new_task_duration > min_time_limit[new_task]`
    - TODO: 下限を`min_batch_size`違反量の増分を調べる
        - 違反量が減るなら挿入する
4. 挿入する
    - `idle_tasks[core].push(cur_tasks[core_id])`
    - `cur_tasks[core_id] = insert task`
    - `q.push((cur_task.next_t.max(insert_task.next_t), ConsumeCore))`

## パケット受け付け
1. `(t, ReceivePacket)`を受け取る
2. パケットを受けとる
3. `packet_type`ごとにタスクを作成して、優先度を計算し直す
4. 空いているコアがある限り優先度が高い順にタスクを割り当てる
    - `cur_tasks[core_id] := Task`
    - `q.push((t, ResumeCore(core_id))`
5. TODO: 残っているタスクで、割り込むべきタスクがあればコアのタスクに差し込む
6. `q.push((t + d, RecievePacket))`

## コアごとの処理 (cur_tasks[core_id] != None)
1. `(t, ResumeCore(core_id))`を受け取る
2. `node_id = 8`なら`works`を問い合わせる
    - TODO: バッチ内に間に合わなそうなパケットが判明したら、分割して処理したり、別のコアに移したりする
        - NOTE: upper_boundで評価しているので、現状は効果がない
        1. `min(timeout) < t + duration`なら間に合わないので、以下をする
        2. `timeout[packet_i] - (t + path_duration[packet_i])`が小さい順にソートする
            - NOTE: worksが判明したのでpath_durationはpacket_iごとに異なる
        3. バッチ内のパケットがtimeoutしないように、前から分割する
        4. `e = min(timeout[packet_i]) - (t + path_duration[batch]) > 0`
        5. 分割したバッチごとに最も良いコアに移す
            - timeoutが最も減るコアを移動先とする
            - NOTE: switch_costがかかるので、基本的には同じコアが良い
        6. `idle_tasks[core_id], cur_tasks[core_id]`に挿入する
3. taskの消費を決める
    - chunkを扱う場合（`s>b || is_chunked`）
        1. `chunk_ids`を作成する
        2. is_advancedを`chunk_ids`について更新する
        3. `dt`を求める
        4. `chunk_ids`でexecuteを投げる
        5. 完了していれば
            - `is_chunked <- false`
            - `is_advanced <- false`
            - `cur_tasks[core_id].path_index += 1;`
    - それ以外の場合
        1. `dt`を求める
        2. `task.ids`でexecuteを投げる
        3. cur_tasks[core_id].path_index += 1;
4. `next_t := t + dt; q.push((next_t, core_id))`

## コアごとの開始処理 (cur_tasks[core_id] == None)
1. `(t, ResumeCore(core_id))`を受け取る
2. `idle_tasks[core_id]`があれば
    - `cur_task[core_id] = idle_task[core_id].pop()`とする
    - `q.push((t, ResumeCore(core_id))`
2. `idle_task`がなければ、最も優先度の高いタスクを割り当てて開始する
3. パケットが残っていなければ、他のコアから分割してタスクをもらってくる
    1. idle_taskがあるならそのままもらってくる
    2. 分割するコア`split_core`を見つける
        - 処理終了時間が最も長いコア
        - TODO: 現状のタスク終了時間が近いコアを選ぶ
        - TODO: 効果が小さいもの（バッチサイズが小さいもの）を弾く
    3. `split_core`のcur_taskを半分に分ける
    4. `cur_task[long_core_id]`と`cur_task[core_id]`を更新する
    5. `q.push((next_t, core_id))`
4. （なければパケットが来るまで待機で良い）

## タスクの作成
1. packet_typeごとに分ける
2. `time_limit[packet_i] := arrive[packet_i] + timeout[packet_i]`が小さい順にソートする
    - 間に合わないことがわかっているものは優先度を一番下げる
3. バッチ内のパケットがtimeoutしないように、バッチサイズを`batch_size_limit=32`まで大きくして、前から分割する
    - `afford = min(time_limit[packet_i]) - (next_t + path_duration[packet_type][s]) > 0`
4. 全てをまとめて、バッチごとに`afford`が小さい順にソートする
