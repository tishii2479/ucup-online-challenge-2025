## パケット受け付け
1. `(t, ReceivePacket)`を受け取る
2. パケットを受けとる
3. `packet_type`ごとにタスクを作成して、優先度を計算し直す
4. 空いているコアがある限り優先度が高い順にタスクを割り当てる
    - `cur_tasks[core_id] := Task`
    - `q.push((t, ResumeCore(core_id))`
5. TODO: 残っているタスクで、割り込むべきタスクがあればコアのタスクに差し込む
    - 条件: タスクを差し込まないとtimeoutが発生する & 差し込むことでtimeoutするpacketが減る
        1. コアごとに差し込むタスクの`departure`を計算する
            - `departure := next_t[core_id] + path_duration[packet_type][1] * (1+a)`
        2. コアごとに差し込まれる`idle_tasks`の新しい`departure[core_id]`を計算する
            - `departure[core_id][idle_i] := departure[idle_i+1] + remaining_duration[core_id] * (1+a)`
        3. timeoutするpacketの個数をコアごとに計算する
        4. コアごとに変化分を計算する
        5. timeoutが最も減るコアを挿入先とする（どのコアも余裕がなければ諦める）
    - 処理: core_idに挿入する
        1. `idle_tasks[core_id].push(cur_tasks[core_id])`
        2. `cur_tasks[core_id] = insert task`
6. `q.push((t + d, RecievePacket))`

## コアごとの処理 (cur_tasks[core_id] != None)
1. `(t, ResumeCore(core_id))`を受け取る
2. `node_id = 8`なら`works`を問い合わせる
    - TODO: バッチ内に間に合わなそうなパケットが判明したら、分割して処理したり、別のコアに移したりする
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
    1. 処理終了時間が最も長いコアを見つける
    2. idle_taskがあるならそのままもらってくる
    3. busiest_coreのcur_taskを半分に分ける
        - TODO:max(処理時間)が最も短くなるような分割を全探索する
    4. `cur_task[long_core_id]`と`cur_task[core_id]`を更新する
    5. `q.push((next_t, core_id))`
4. （なければパケットが来るまで待機で良い）

## タスクの作成
1. packet_typeごとに分ける
2. `time_limit[packet_i] := arrive[packet_i] + timeout[packet_i]`が小さい順にソートする
    - TODO: 間に合わないことがわかっているものは優先度を一番下げる
    - TODO: durationはspecial_nodeとそれ以外で分けて評価する
3. バッチ内のパケットがtimeoutしないように、バッチサイズを一定サイズまで大きくして、前から分割する
    - `afford = min(time_limit[packet_i]) - (t + path_duration[packet_type][s]) > 0`
    - TODO: バッチサイズの上限を設ける
4. 全てをまとめて、バッチごとに`afford`が小さい順にソートする
