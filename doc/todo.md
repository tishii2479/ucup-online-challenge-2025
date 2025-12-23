2%伸ばす
1%: パラメータチューニング & 細かい工夫
1%:

## TODO
must
- timeout: insertをうまくやる
    - ノード8のinsertを実装する
- throughput: 空き時間を減らす
    - コアの選択基準を工夫する
        - 正規化して評価関数を頑張る
        - 基本は最大長のものを選ぶ
        - dtが大きすぎる場合は、近くで選べるものを選ぶ
    - chunkされたバッチを持っていくの確認

want
- taskをマージした方が良い場合は、パケットを追い付かせてマージさせる
    - 全体が遅れるので、少し負の評価をつける

## LAST
- TRACK=false
- 安全なawait_intervalを提出する
- batch-size, alphaを変えてたくさん提出する
    - パラメータのチューニング
        - `max_batch_size`: `n_cores, n, packet_type` ごとに良いバッチサイズがあるはず
        - `alpha`
- 最後はbest provisional scoreを提出する

## others.
- 暫定テストケースを探る
