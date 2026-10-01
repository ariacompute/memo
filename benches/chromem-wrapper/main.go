// Command chromem is a minimal CLI wrapper around chromem-go, used as a local
// embedded control group for the memo Track A benchmark harness.
//
// The benchmark adapter (benches/adapters/chromem.py) drives this binary as a
// subprocess with the following surface:
//
//	chromem <db> add        --content=<text>
//	chromem <db> add-batch  --file=<path>            # JSONL of strings (`-` = stdin)
//	chromem <db> query      --text=<text> --top-k=<k>     # prints `score\tcontent` per line
//	chromem <db> query-batch --file=<path> --top-k=<k>    # JSONL of queries; one JSON array per line
//
// <db> is a directory used for chromem's persistent store, so state survives
// across the per-call subprocess invocations the harness spawns.
//
// Embeddings are produced locally (hashing n-gram bag-of-words + L2 norm) — no
// model download, fully offline. Swap embedFn for a real offline embedder if you
// want a stronger control-group signal.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"hash/fnv"
	"io"
	"math"
	"os"
	"strconv"
	"strings"
	"unicode"

	chromem "github.com/philippgille/chromem-go"
)

const (
	collectionName = "benches"
	embedDim       = 256
)

func main() {
	if len(os.Args) < 3 {
		fmt.Fprintln(os.Stderr, "usage: chromem <db> <add|add-batch|query|query-batch> [flags]")
		os.Exit(2)
	}
	dbPath := os.Args[1]
	cmd := os.Args[2]
	args := os.Args[3:]

	if err := run(dbPath, cmd, args); err != nil {
		fmt.Fprintln(os.Stderr, "chromem:", err)
		os.Exit(1)
	}
}

func run(dbPath, cmd string, args []string) error {
	// chromem persistent store expects a directory; MkdirAll also tolerates the
	// harness passing a file-shaped path like `.../chromem.db`.
	if err := os.MkdirAll(dbPath, 0o700); err != nil {
		return err
	}

	switch cmd {
	case "add":
		content, ok := getFlag(args, "content")
		if !ok || content == "" {
			return fmt.Errorf("add requires --content=")
		}
		db, err := chromem.NewPersistentDB(dbPath, false)
		if err != nil {
			return err
		}
		coll, err := db.GetOrCreateCollection(collectionName, nil, embedFn)
		if err != nil {
			return err
		}
		// deterministic id from content; retry with suffix on duplicate-id error
		id := fmt.Sprintf("m%x", fnv32(content))
		for attempt := 0; ; attempt++ {
			// embeddings=nil -> chromem computes via embedFn; metadatas=nil
			if err = coll.Add(context.Background(), []string{id}, nil, nil, []string{content}); err == nil {
				break
			}
			if attempt >= 5 {
				return err
			}
			id = fmt.Sprintf("m%x-%d", fnv32(content), attempt)
		}
		fmt.Println(id)
		return nil

	case "add-batch":
		// Batched add: reads contents as JSONL (one JSON-encoded string per line,
		// or raw text lines) from --file=<path> (`-` for stdin) and inserts them
		// all in a single process. This avoids the per-item `add` subprocess spawn
		// storm that made large corpora (10k/100k) unusably slow in the harness.
		file, ok := getFlag(args, "file")
		if !ok || file == "" {
			return fmt.Errorf("add-batch requires --file=<path> (JSONL of strings; use '-' for stdin)")
		}
		contents, err := readLines(file)
		if err != nil {
			return err
		}
		if len(contents) == 0 {
			return fmt.Errorf("add-batch: no contents read from %s", file)
		}
		db, err := chromem.NewPersistentDB(dbPath, false)
		if err != nil {
			return err
		}
		coll, err := db.GetOrCreateCollection(collectionName, nil, embedFn)
		if err != nil {
			return err
		}
		// deterministic ids from content (matches the per-item `add` id scheme).
		// chromem computes embeddings via embedFn (pass nil); metadatas=nil.
		ids := make([]string, len(contents))
		for i, c := range contents {
			ids[i] = fmt.Sprintf("m%x", fnv32(c))
		}
		if err = coll.Add(context.Background(), ids, nil, nil, contents); err != nil {
			return err
		}
		fmt.Printf("added %d\n", len(contents))
		return nil

	case "query":
		text, ok := getFlag(args, "text")
		if !ok || text == "" {
			return fmt.Errorf("query requires --text=")
		}
		topK := 5
		if v, ok := getFlag(args, "top-k"); ok {
			if n, err := strconv.Atoi(v); err == nil && n > 0 {
				topK = n
			}
		}
		db, err := chromem.NewPersistentDB(dbPath, false)
		if err != nil {
			return err
		}
		coll, err := db.GetOrCreateCollection(collectionName, nil, embedFn)
		if err != nil {
			return err
		}
		res, err := coll.Query(context.Background(), text, topK, nil, nil)
		if err != nil {
			return err
		}
		for _, r := range res {
			// higher Similarity = more similar (range [-1,1]); adapter reads score
			fmt.Printf("%f\t%s\n", r.Similarity, r.Content)
		}
		return nil

	case "query-batch":
		// Batched query: reads queries as JSONL (one JSON-encoded string per line,
		// or raw text) from --file=<path> (`-` for stdin) and runs them ALL in a
		// single process with the DB loaded once. This mirrors `add-batch` and
		// avoids the per-query subprocess spawn storm (one DB load per query) that
		// made 10k/100k search corpora in the harness take tens of minutes to hours.
		file, ok := getFlag(args, "file")
		if !ok || file == "" {
			return fmt.Errorf("query-batch requires --file=<path> (JSONL of query strings; use '-' for stdin)")
		}
		topK := 5
		if v, ok := getFlag(args, "top-k"); ok {
			if n, err := strconv.Atoi(v); err == nil && n > 0 {
				topK = n
			}
		}
		queries, err := readLines(file)
		if err != nil {
			return err
		}
		if len(queries) == 0 {
			return fmt.Errorf("query-batch: no queries read from %s", file)
		}
		db, err := chromem.NewPersistentDB(dbPath, false)
		if err != nil {
			return err
		}
		coll, err := db.GetOrCreateCollection(collectionName, nil, embedFn)
		if err != nil {
			return err
		}
		for _, q := range queries {
			res, err := coll.Query(context.Background(), q, topK, nil, nil)
			if err != nil {
				return err
			}
			lines := make([]queryResult, 0, len(res))
			for _, r := range res {
				lines = append(lines, queryResult{Score: float64(r.Similarity), Content: r.Content})
			}
			b, err := json.Marshal(lines)
			if err != nil {
				return err
			}
			fmt.Println(string(b))
		}
		return nil

	default:
		return fmt.Errorf("unknown command %q (expected add|add-batch|query|query-batch)", cmd)
	}
}

// queryResult is the JSON shape emitted by `query-batch` for each query: one
// JSON array of these per line, so the adapter can reconstruct a SearchHit list.
type queryResult struct {
	Score   float64 `json:"score"`
	Content string  `json:"content"`
}

// readLines reads contents from a file path, or stdin when path is "-".
// Each line is a JSON-encoded string (so quoting/escaping is handled), with a
// raw-text fallback if a line is not valid JSON. Blank lines are skipped.
func readLines(path string) ([]string, error) {
	var r io.Reader
	if path == "-" {
		r = os.Stdin
	} else {
		f, err := os.Open(path)
		if err != nil {
			return nil, err
		}
		defer f.Close()
		r = f
	}
	data, err := io.ReadAll(r)
	if err != nil {
		return nil, err
	}
	var out []string
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		var s string
		if err := json.Unmarshal([]byte(line), &s); err != nil {
			out = append(out, line) // raw-text fallback
			continue
		}
		out = append(out, s)
	}
	return out, nil
}

// embedFn is a fully offline, dependency-free embedding: tokenize (alphanumeric
// unigrams + CJK unigrams/bigrams), hash each token into a fixed-dim vector with
// term-frequency weighting, then L2-normalize. Must be passed to
// GetOrCreateCollection — passing nil would fall back to the OpenAI embedder.
func embedFn(_ context.Context, text string) ([]float32, error) {
	vec := make([]float32, embedDim)
	toks := tokenize(text)
	if len(toks) == 0 {
		return vec, nil
	}
	tf := make(map[uint32]float32, len(toks))
	for _, t := range toks {
		tf[fnv32(t)] += 1
	}
	for h, c := range tf {
		vec[h%embedDim] += c / float32(len(toks))
	}
	var norm float32
	for _, v := range vec {
		norm += v * v
	}
	if norm > 0 {
		n := float32(math.Sqrt(float64(norm)))
		for i := range vec {
			vec[i] /= n
		}
	}
	return vec, nil
}

func tokenize(text string) []string {
	lower := strings.ToLower(text)
	var toks []string
	var buf strings.Builder
	flush := func() {
		if buf.Len() > 0 {
			toks = append(toks, buf.String())
			buf.Reset()
		}
	}
	runes := []rune(lower)
	for i, r := range runes {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			buf.WriteRune(r)
		} else {
			flush()
		}
		if unicode.Is(unicode.Han, r) {
			toks = append(toks, string(r))
			if i+1 < len(runes) && unicode.Is(unicode.Han, runes[i+1]) {
				toks = append(toks, string(r)+string(runes[i+1]))
			}
		}
	}
	flush()
	return toks
}

func fnv32(s string) uint32 {
	h := fnv.New32a()
	_, _ = h.Write([]byte(s))
	return h.Sum32()
}

// getFlag parses `--name=value` style flags from the argument list.
func getFlag(args []string, name string) (string, bool) {
	pref := "--" + name + "="
	for _, a := range args {
		if strings.HasPrefix(a, pref) {
			return strings.TrimPrefix(a, pref), true
		}
	}
	return "", false
}
