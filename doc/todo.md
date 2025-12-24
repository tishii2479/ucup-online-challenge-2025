## TODO
- insert-taskを追加する
    - create-taskで残ったタスクを使う
    - 割り当てられなかったタスクの上位k個をcore_idに挿入することを試す
    - min(diff e_timeout) < 0 なら、そのcore_idに挿入する
        - `insert_task_start_t = cur_task.next_t.max(insert_task.next_t)`
        - `insert_task_end_t = insert_task_start_t + insert_task_duration`
        - `cur_task_end_t = insert_task_end_t + cur_task_duration`
        - `min_insert_task_e_timeout = min(start_ts)`
    - 挿入
        - `idle_task[core_id] = next_task[core_id]`
        - `next_task[core_id] = insert-task`
        - `q.push((insert_task_start_t, ConsumeCore))`

- node=8にinsertを追加する
- await-packetsの高速化

- パラメータ
    - batch-size
        - n, n_cores, packet-typeごとに変える
    - alpha


## LAST
- TRACK=false, eprintlnを消す
- 安全なawait_intervalを提出する
- 安全なthresholdで提出する
- batch-size, alphaを変えて提出する
- 最後はbest provisional scoreを提出する

## want.
- create-taskでも期待値を計算する
    - e_timeout := \sum e_timeout / n
    - contition: e_timeout < th
        - th := 0.01
    - priority: e_timeout
