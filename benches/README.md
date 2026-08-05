# Benches

业界对比评测编排。

| Track | 内容 | 默认依赖 |
|-------|------|----------|
| **A** | 微基准（延迟/吞吐）+ 合成检索 Recall@k / MRR | `aria-memo` CLI；他系统按 SDK |
| **B** | LoCoMo / LongMemEval / BEAM 端到端 | 外部 LLM + 可选云 API |

对比系统：`aria` / `mem0` / `memos` / `mempalace` / `zep` / `letta`。

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
python benches/run.py --track b --benchmarks locomo,longmemeval,beam --systems aria
python benches/run.py --track all --size 500 --dry-run
```

环境变量（Track B / 他系统）：

| 变量 | 用途 |
|------|------|
| `ARIA_MEMORY_BIN` | aria-memo 可执行文件路径 |
| `OPENAI_API_KEY` / `BENCH_LLM_*` | 答/判 LLM（B） |
| `MEM0_API_KEY` | mem0 托管 |
| `MEMOS_*` | MemOS |
| `ZEP_API_KEY` | Zep |
| `LETTA_*` | Letta |

缺密钥时 adapter **skip** 并写入 `reason`，不伪造分数。

## 布局

```
benches/
  run.py
  requirements.txt
  common/          # 计时、报告
  adapters/        # 统一 MemoBackend 接口
  track_a/         # 微基准 + 合成检索
  track_b/         # LoCoMo / LongMemEval / BEAM
  data/            # synthetic_retrieval.json
  results/         # 运行产物
```

## OmniMemEval / memo-benchmarks

Track B runner 输出与 OmniMemEval User Memo（`add`/`search` adapter）同形契约。可将本仓库 `adapters/` 接到：

- https://github.com/MemTensor/OmniMemEval
- https://github.com/mem0ai/memo-benchmarks

详见各 adapter 模块文档字符串。

## 功能矩阵

见仓库 [`docs/compare.md`](../docs/compare.md)。
