# Track B 数据集

仓库遵循「零网络依赖、不入库大体量数据」红线：仅 `fixtures/<bench>/` 内置极小合成样例（供单测与离线冒烟），真实数据集由用户手动放置或通过 `python benches/datasets.py --download <bench>` 自动拉取。

## 各基准目录约定与上游地址

### locomo_refined（CC BY-NC 4.0，仅研究用途）
- 上游：https://github.com/mem-eval-suite/LoCoMo_refined
- 放置：
  ```
  benches/data/locomo_refined/questions.jsonl
  benches/data/locomo_refined/conversations.jsonl
  ```

### halumem
- 上游（HF）：https://huggingface.co/datasets/IAAR-Shanghai/HaluMem
  `huggingface-cli download IAAR-Shanghai/HaluMem --local-dir benches/data/halumem`
- 放置：
  ```
  benches/data/halumem/sessions.jsonl
  benches/data/halumem/memories.jsonl
  benches/data/halumem/questions.jsonl
  ```

### longmemeval
- 上游：https://github.com/xiaowu0162/LongMemEval
- 放置（至少提供 S 变体）：
  ```
  benches/data/longmemeval/longmemeval_s.json
  benches/data/longmemeval/longmemeval_m.json        # 可选
  benches/data/longmemeval/longmemeval_oracle.json   # 可选
  ```

### personamem
- 上游：https://github.com/bowen-upenn/PersonaMem
- 放置：
  ```
  benches/data/personamem/shared_contexts_32k.jsonl
  benches/data/personamem/questions_32k.csv
  benches/data/personamem/shared_contexts_128k.jsonl  # 可选
  benches/data/personamem/questions_128k.csv          # 可选
  benches/data/personamem/shared_contexts_1M.jsonl    # 可选
  benches/data/personamem/questions_1M.csv            # 可选
  ```

## 缺失处理

`resolve_dataset(bench)` 优先真实目录，缺失则回退 `fixtures/`，并在报告中标注 `dataset_source: fixture`（防止把 fixture 分数误当正式结果）。两者皆缺失时抛 `FileNotFoundError` 并打印手动下载指引，不静默失败。

## 许可提示

- LoCoMo-Refined：CC BY-NC 4.0，仅限研究用途，商用需另行授权。
- 其余基准请遵循各自上游许可与署名要求。
