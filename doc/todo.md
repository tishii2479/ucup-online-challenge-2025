## TODO
- insert-task
- create-taskでも期待値を計算する
- パラメータ
    - batch-size
        - n, n_cores, packet-typeごとに変える
    - alpha

## 変数
- select-task
    - batch-size
    - packet-type
- switch-task
    - from-core
    - split-size
- insert

## LAST
- TRACK=false, eprintlnを消す
- 安全なawait_intervalを提出する
- 安全なthresholdで提出する
- batch-size, alphaを変えて提出する
- 最後はbest provisional scoreを提出する
