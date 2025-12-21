## TODO:
- 入力分布の特定
    - 順位表を使って分布を探る（assert）
- 可視化

- 最後のパケットの到着時刻でスコアが変わる
- timeoutがきつければ追い越すことができる
- 序盤は適当に割り振るで良いなら、後半は最適化できる
    - 種類ごとにおおよそ固定すると良さそう
- ジョブを始めるタイミング
    - 前のジョブが終わったら
    - 新しいパケットが届いたら
- 0-indexed, 1-indexed
- switchを減らすために、同じパケットはなるべく同じコアで処理したい
- コアは一つのノードでしか作業できない
- 基本的にコアの切り替えは少ないほどよい
    - 基本的にはパケットごとに見れば同じコアを使い続けるのが良さそう & そうなりそう
    - 暇な時に割り当てるくらいで良さそう
- ノード8がある & 入力がいつ来るかわからないことを考えると、まとめて最適化するのは筋が悪そう
- コアごとに見れば最も良いものは割と単純に選べそう
    - ノード -> timeoutまでの時間が短い順

## 課題
- いつ読み込むか
    - ペナルティがないなら`c_r`ごとに読み込めば良さそうだが、時間が進むので他のアクションがしづらくなる
    - 操作をしている途中で、読み込みを差し込めるなら都度差し込む
- どのコアを使うか
    - coreごとにイベントを管理すると最適なコアを使えない
    - jobが開始できる時に拾えるパケットを見れば良い
    - 最初だけ面倒かも？
- 少し待った方が良い場合
    - 実際に少し待った場合の評価もする
- 操作の評価
- どうやってバッチにまとめるか
    - 10~20個くらいにまとめれば十分効果がある
- ノード8のコストをどう扱うか
- 実行時間をどう使うか
    - 2sしかない
    - nも5e3くらい
    - 多分大掛かりな最適化はできない

## 解決策
- いくつか工夫すればまとめて焼けそう
- タスクの順序を焼きなます

## タスクの分割
`is_chunked`ではないなら
- idsだけを分ける

`is_chunked`なら
- !is_advanced[i]の個数を比例するように分ける
- !is_advancedが一つもないならis_chunked=false
- idsはis_advancedとの対応に気をつけながら分ける

```
order[core] := [
    Job { job_index, node_index, [(packet_index, path_index)] }
]
job_graph[index]: [dep_jobs]
job_graph[index] {
    同じコアの次のジョブ,
    (packet_index, path_index + 1)を含むジョブ
}
```

- core間のswap, move
- (packet_index, path_index)のjob間の移動, swap
- core内のjobのswap, move

緩和問題を考える
- ノード8がない
- 後から入ってこない

大きな課題
- timeoutをなくす
- 最後をうまくやる
- 稼働していない期間を減らす
    - バッチの分割待ちを減らせれば良い
- ノード8をうまく対処する
- 後から入ってくるものをうまく処理する

- コア間の入れ替わりがないなら、入れ替わりで作業をする必要がない

- stack or queue?
    - priority(idle_task) < priority(core_id) となっているはず
    - stackで良いはず

タスクの作成・優先度
- バッチサイズは一定サイズまで大きい方が良い
- timeoutが間に合う範囲で大きくしている
- たまに他のタスクの開始時刻が遅くなるが、平均で見れば良いはず
- IDEA:
    - packet_typeごとに良いバッチサイズがあるはず
    - バッチサイズが偏らない方が良いはず
        - 32,32,1より22,22,21
    - コアが少ない場合(=1,2)を考えた方が良いかも
    - ギリギリのパケットを捨てた方が良いケースがあるかも

insert
- タスクの作成も変える必要があるかも: insertする場合には、小さいタスクの方が良い場合がある
    - insertした後で、idle-taskが間に合う必要がある
    - insertして間に合うなら、後でも間に合う？
        - そんなことない
    - あんまり小さいと全体に悪影響が出る
        - min_batch_sizeを導入して、それ未満なら挿入は諦める
- ノード8が判明したことで、挿入できるようになる場合がある
- coreごとに挿入できる`afford`がある
    - `afford_duration := min_time_limit[cur_task] - current_end_time`
    - `next_t + duration <= min_time_limit[new_task]`
    - `duration < afford_duration`になるようにタスクを作成する
    - 多少`cur_task`でtimeoutが出ても、他のコアが引き取ってくれる可能性がある
    - 他のコアの`idle_task`に移すことで、優先的に処理できるかも
- 一般化すると、連鎖的に挿入することになる
    - 基本的に移動が増えると効率が落ちるので、そこまで広い連鎖にはならないはず
- 今はtimeoutしないバッチだけ割り当てている

- lbとubが結構離れている
- ノード8が判明したタイミングで挿入できる可能性が高そう

## 貪欲を考えよう

input: packetの集合
output: [(priority, batch)]

評価: `dt*a - 1e4*(timeout/n) (0<a<1)`
変数
- packet_typeごとの分割数
- packet_typeごとの分割位置

### バッチの分割
- `time_limit`ごとにソートする
- もう間に合わないものは無視する
- timeoutするようになる前にバッチを切る
    - TODO: 後半は分割されるかもなので、無駄になっている可能性がある
    - TODO: timeoutを緩和

a. パケットがもう来ない場合
- packet_typeごとにバッチサイズを均等に分ける
    - `batch_count[type] = ceil(b[type] / BATCH_SIZE)`
    - `batch_size[type] = ceil(b / batch_count[type])`

b. パケットがまだ来る場合
- `batch_size`ごとに分割する

### バッチの優先度
- min_time_limitまでの時間
- 最適解ではないだろうが、それなりに良い貪欲になっているはず


## 課題
目的: timeoutの最小化、durationの最小化

- durationだけを意識するなら、バッチサイズは30くらいに近いほどよい
    - 小さいバッチをinsertをすると全体に悪影響が出る
- 小さいのを間に合わせるのは、後からまだパケットが来るなら非効率
    - timeoutは減らせるかも
- 小さいのに大きいのを間に合わせるのは、ちょっとだけ得できるが
    - が、そもそも小さいのはあまり時間がかからないので、timeoutにはあまり影響がない

- insertをする悪影響を評価できない
- catch-upをする悪影響を評価できない
- 貪欲に前から作ると後で損をすることがある
    - 後ろのpacketが多くtimeoutするかも
    - 小さいバッチサイズで作成することで、全体が遅れる可能性がある
- パケットがなくなった時に、他のタスクが終わる待ち時間が発生する
- 1->2->3だと、1->3に比べて切り替え時間が余分にかかる
- `special_cost`の推定ができないので、上界で過剰評価している

- insert: timeout +, duration -
- catch-up: timeout +e, duration +e
- 貪欲改善: timeout +, duration +
- 待ち時間を減らす: timeout ., duration +
- switch-cost: timeout ., duration +
- special-cost: timeout +, duration .

1. 待ち時間を減らす
2. 貪欲の改善
3. catch-up
4. special-cost, switch-cost

- special_costは平均取ったら分散が小さそう

## 課題
- 序盤のtimeoutを減らす
    - insert, catch-up
- 空き時間を減らすような工夫
- timeoutを緩和、良いバッチを作る
    - packetを全て受け取ってから、コアにタスクを割り当てる度にタスクの順序を最適化

input: {
    packet_type: [min_time_limit; k]
    [core_end_t; n_cores]
}
state: packet_typeの順序, packet_typeごとのバッチの区切り目
score: シミュレーションをした後のスコア（node_id=8は推定値）
neighbor: packet_typeのswap, packet_typeごとの区切り目の変更


insertがうまくいかないのは、現状のタスクがtimeoutギリギリに作られているからかも
分割して先に処理する実装をすればinsertが有効になるかも

局所探索はコストリターンが合わなそう

## insert
1. packet_typeごとに`(cur_task.s / 2).min(n_packet_type)`のバッチを作る
    - time_limitが近い順にpacketを選ぶ
    - そもそも間に合わないものは選択しない
    - そのままだとtimeoutするパケットがなければ挿入しなくて良い
2. packet_typeごとに挿入した後で、元のバッチを進めて、d_timeoutとdtを求める
    - 途中で半分くらいに分割しても良い
        - 分割するタイミング・有無は全探索する
    - 2つのタスクに分ける
3. `(d_timeout, dt)`でコアごとにソートする
4. 各コアについて1つを上限で、`d_timeout < 0`を採用する

## annealing
1. worksが全て判明するタイミングを調べる

12/21
12/22 insert
12/23 -
12/24 最終調整
