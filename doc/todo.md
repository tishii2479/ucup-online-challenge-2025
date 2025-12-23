2%伸ばしたい!


## TODO
must
- バッチの作り方
    - optimize-task
    - create-task
- taskをもらってくるところの改善
    - 終わった時に実際にタスクを渡す
    - タスクを持ってくる前に、終わる前に終わらせられるタスクを先にやる
- パラメータ
    - batch-size
    - alpha
- 時間の管理
    - thresholdをいくつか提出する

want
- insertをうまくやる

## LAST
- TRACK=false, eprintlnを消す
- 安全なawait_intervalを提出する
- batch-size, alphaを変えてたくさん提出する
    - パラメータのチューニング
        - `max_batch_size`: `n_cores, n, packet_type` ごとに良いバッチサイズがあるはず
        - `alpha`
- 最後はbest provisional scoreを提出する
