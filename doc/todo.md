## TODO
must
- timeout: insertをうまくやる
    - ノード8のinsertを実装する
- throughput: 空き時間を減らす
    - コアの選択基準を工夫する
        - 正規化して評価関数を頑張る
        - 基本は最大長のものを選ぶ
        - dtが大きすぎる場合は、近くで選べるものを選ぶ
    - chunkされたバッチを持ってくる
        - chunkの状態更新を治す
            - 最後のチャンクを処理している時にis_chunked = falseになっている
            - ので、チャンクの残りを持ってくることができない
    - 先読みしてタスクをチャンクに分けておく

want
- 途中までバッチで処理して、間に合わないパケットだけ分割して処理する
- taskをマージした方が良い場合は、パケットを追い付かせてマージさせる
    - 全体が遅れるので、少し負の評価をつける

## IDEA
- switch-costを最小化する

## LAST
- TRACK=false
- dt、batch-size, alphaを変えてたくさん提出する
    - 最後は安全なdt or best provisional scoreを提出する
    - receive_packetのdtの調整
        - 安全なdtで提出する
    - パラメータのチューニング
        - `max_batch_size`: `n_cores, n, packet_type` ごとに良いバッチサイズがあるはず

## others.
- 暫定テストケースを探る
