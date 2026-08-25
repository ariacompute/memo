# Benches

业界对比评测编排。

| Track | 内容 | 默认依赖 |
|-------|------|----------|
| **A** | 微基准（延迟/吞吐）+ 合成检索 Recall@k / MRR | `aria-memo` CLI；他系统按 SDK |
| **B** | 四基准端到端：locomo_refined / halumem / longmemeval / personamem | 离线指标零网络；judge 指标需 OpenAI 兼容 LLM（缺则 skip） |

对比系统：`aria` / `mem0` / `memos` / `mempalace` / `zep` / `letta`。

### Track B 四基准一览

| 基准 | 数据 | 离线指标（零网络可出） | judge 指标（需 LLM 凭据） |
|------|------|------------------------|----------------------------|
| locomo_refined | questions.jsonl + conversations.jsonl | token-F1 / BLEU（按 category 分组） | 严格 judge 准确率 |
| halumem | sessions/memories/questions.jsonl | 检索 Recall@5（提取/QA） | 提取 Recall/Accuracy/FMR/F1；更新准确率/幻觉率；QA 准确率/幻觉率/遗漏率 |
| longmemeval | longmemeval_{s,m,oracle}.json | 检索 Recall@5 | QA 准确率 |
| personamem | questions_{32k,128k,1M}.csv + shared_contexts_*.jsonl | 多选准确率（完全离线） | — |

**judge 可选红线**：所有依赖 LLM 判定的指标仅在配置了 `BENCH_LLM_API_KEY`（或 `OPENAI_API_KEY`）+ `BENCH_LLM_BASE_URL`/`BENCH_LLM_MODEL` 时计算；否则该指标 `skipped` 并写 `reason`，**不伪造分数**。离线指标（F1/BLEU/多选/Recall@k）在无数据集/无 LLM 时经 `data/fixtures/` 内置合成样例仍可跑通单测与冒烟。

## 安装

```bash
# 仓库根目录
cargo build -p aria-memo --release
pip install -r benches/requirements.txt
export ARIA_MEMORY_BIN="$(pwd)/target/release/aria-memo"   # 可选
```

## 运行

```bash
python benches/run.py --track a --size 1000 --systems aria
python benches/run.py --track a --size 1000 --systems aria,mem0,memos,mempalace,zep,letta
python benches/run.py --track b --dry-run
python benches/run.py --track b --benchmarks locomo_refined,halumem,longmemeval,personamem --backend aria
python benches/run.py --track b --download            # 自动拉取缺失数据集
python benches/run.py --track b --limit 50            # 采样上限，避免大集跑爆内存
python benches/run.py --track b --judge-model gpt-4o-mini   # 显式指定 judge 模型
python benches/run.py --track all --size 500 --dry-run
```

环境变量（Track B / 他系统）：

| 变量 | 用途 |
|------|------|
| `ARIA_MEMORY_BIN` | aria-memo 可执行文件路径 |
| `BENCH_LLM_API_KEY` / `OPENAI_API_KEY` | judge LLM 凭据（Track B judge 指标） |
| `BENCH_LLM_BASE_URL` | OpenAI 兼容 base url（默认官方） |
| `BENCH_LLM_MODEL` / `--judge-model` | judge 模型名（报告标注实际模型） |
| `MEM0_API_KEY` | mem0 托管 |
| `MEMOS_*` | MemOS |
| `ZEP_API_KEY` | Zep |
| `LETTA_*` | Letta |

缺 judge 密钥时 judge 指标 **skip** 并写入 `reason`，不伪造分数；离线指标照常出分。其他系统 adapter 不可用时 skip 并写 `reason`。

## 布局

```
benches/
  run.py
  requirements.txt
  common/          # 计时、报告、Score 结构
  metrics/         # 离线指标：F1 / BLEU / 多选 / Recall@k
  judge.py         # OpenAI 兼容 judge 客户端（可选）
  datasets.py      # 数据集定位与下载
  adapters/        # 统一 MemoBackend 接口（含可选能力 list_memories/update）
  track_a/         # 微基准 + 合成检索
  track_b/         # 四基准注册表（locomo_refined/halumem/longmemeval/personamem）
  data/            # synthetic_retrieval.json + fixtures/<bench>/ 内置合成样例
  results/         # 运行产物
```

## OmniMemEval / memo-benchmarks

Track B runner 输出与 OmniMemEval User Memo（`add`/`search` adapter）同形契约。可将本仓库 `adapters/` 接到：

- https://github.com/MemTensor/OmniMemEval
- https://github.com/mem0ai/memo-benchmarks

详见各 adapter 模块文档字符串。

## 功能矩阵

见仓库 [`docs/compare.md`](../docs/compare.md)。
