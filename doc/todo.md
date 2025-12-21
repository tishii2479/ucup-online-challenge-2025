## TODO
must
- insertをうまくやる
    - ノード8のinsertを実装する
- 空き時間を減らす
    - コアの選択基準を工夫する
        - 基本は最大長のものを選ぶ
        - 長すぎる場合は、近くで選べるものを選ぶ
    - chunkされたバッチを持ってくる
        - chunkの状態更新を治す
    - 先読みしてタスクをチャンクに分けておく
- `duration`、`special`の推定を賢くする
    - `works`が溜まってきたら`special_cost_estimate`を再計算する

want
- 途中までバッチで処理して、間に合わないパケットだけ分割して処理する
- taskをマージした方が良い場合は、パケットを追い付かせてマージさせる
    - 全体が遅れるので、少し負の評価をつける

## IDEA

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
