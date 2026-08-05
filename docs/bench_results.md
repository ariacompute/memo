# 评测结果说明

## 如何复现

```bash
cargo build -p aria-memo --release
export ARIA_MEMORY_BIN="$(pwd)/target/release/aria-memo"
pip install -r benches/requirements.txt

python benches/run.py --track a --size 1000 --systems aria
python benches/run.py --track a --size 1000 --systems aria,mem0,memos,mempalace,zep,letta
python benches/run.py --track b --dry-run --systems aria,mem0,memos,mempalace,zep,letta
python benches/run.py --track b --benchmarks locomo,longmemeval,beam --systems aria
```

结果目录：`benches/results/<timestamp>/`（`track_a.json|md`、`track_b.json|md`）。

## 本地样例（aria，size=100，本机一次跑通）

| 指标 | 数值 |
|------|------|
| add p50 / p99 | ~4.0 ms / ~7.1 ms |
| search p50 / p99 | ~0.13 ms / ~0.37 ms |
| add / search ops/s | ~236 / ~7387 |
| 合成检索 Recall@5 / MRR | 1.0 / 1.0 |
| offline / includes_network | true / false |

他系统未装 SDK / 未配密钥时为 `skipped` + `reason`（不伪造分数）。功能定性见 [compare.md](./compare.md)。

## 解读注意

1. **A 层**才是 aria 与托管产品同口径主战场（延迟、离线、包体）。
2. **B 层**依赖抽取与 Judge 模型；`--dry-run` 只验证管线骨架。
3. aria 微基准走 `memo bench --json`（进程内），不含进程启动摊销。
