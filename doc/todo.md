## TODO
- create-taskでも期待値を計算する
    - e_timeout := \sum e_timeout / n
    - contition: e_timeout < th
        - th := 0.01
    - priority: e_timeout
- insert-taskを追加する
    - create-taskでタスクを作る
    - 割り当てられなかったタスクの上位k個をcore_idに挿入することを試す
    - min(diff e_timeout) < 0 なら、そのcore_idに挿入する
- node=8にinsertを追加する

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
