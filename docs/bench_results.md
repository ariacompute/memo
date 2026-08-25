# 评测结果说明

## 如何复现

```bash
cargo build -p aria-memo --release
export ARIA_MEMORY_BIN="$(pwd)/target/release/aria-memo"
pip install -r benches/requirements.txt

python benches/run.py --track a --size 1000 --systems aria
python benches/run.py --track a --size 1000 --systems aria,mem0,memos,mempalace,zep,letta
python benches/run.py --track b --dry-run --backend aria
python benches/run.py --track b --backend aria                 # 离线指标，judge 自动 skip
export BENCH_LLM_API_KEY=...                                   # 可选：启用 judge 指标
python benches/run.py --track b --judge-model gpt-4o-mini
python benches/run.py --track b --download                     # 拉取缺失数据集
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

## 指标口径

| 指标 | 来源 | 是否需要 LLM |
|------|------|--------------|
| token-F1 / BLEU | locomo_refined | 否（离线） |
| 多选准确率 | personamem | 否（离线，选项匹配） |
| 检索 Recall@5 | halumem / longmemeval | 否（离线） |
| judge 准确率 / 提取 Recall·Accuracy·FMR / 更新准确率·幻觉率 / QA 幻觉率·遗漏率 | 各基准 | 是（OpenAI 兼容 judge，缺凭据则 skip） |

> **dataset_source**：若使用 `data/fixtures/` 内置合成样例，报告标注 `fixture`，分数仅供管线验证，不可与正式基准分数直接对比。

## 解读注意

1. **A 层**才是 aria 与托管产品同口径主战场（延迟、离线、包体）。
2. **B 层**离线指标无需 LLM 即可真跑；judge 指标依赖模型，缺凭据时 `skipped` + `reason`，不伪造分数。
3. aria 微基准走 `memo bench --json`（进程内），不含进程启动摊销。
