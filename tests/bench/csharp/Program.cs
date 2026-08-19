using System.Diagnostics;
using System.Numerics;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace DreamBench;

/// <summary>
/// 1:1 C# port of tests/bench/microbenches.dream for side-by-side ns/op comparison.
/// Pass --dream-scores path/to/native.txt (from run-microbenches.sh) for live ratios.
/// Dream runs under Wasm/c + ARC; this is native JIT + GC — substrate differs.
/// </summary>
public static class Program
{
    static readonly Dictionary<string, long> DreamScores = new();
    static bool IsWarmup = true;
    static int Sink;

    // Index-based scratch slab (mirrors Dream ScratchArena.bump / set_at / at).
    sealed class ScratchArena<T>
    {
        readonly T[] items;
        int used;

        public ScratchArena(int capacity)
        {
            items = new T[Math.Max(1, capacity)];
            used = 0;
        }

        public int Bump(int n)
        {
            if (n < 0 || used + n > items.Length)
                throw new InvalidOperationException("ScratchArena overflow");
            int start = used;
            used += n;
            return start;
        }

        public void SetAt(int index, T value) => items[index] = value;
        public T At(int index) => items[index];
        public void Reset() => used = 0;
    }

    static void Report(string name, long elapsedNanos, int iters)
    {
        if (IsWarmup) return;
        long csharpNs = elapsedNanos / iters;
        Console.WriteLine($"bench {name} ns_per_op={csharpNs}");
        if (DreamScores.TryGetValue(name, out long dreamNs) && dreamNs > 0)
        {
            double ratio = (double)csharpNs / dreamNs;
            string cmp = ratio > 1.0
                ? $"C# is {ratio:F1}x slower"
                : $"C# is {(1.0 / ratio):F1}x faster";
            Console.Error.WriteLine($"  compare {name,-18} C#={csharpNs,6} Dream={dreamNs,6} | {cmp}");
        }
    }

    static long ElapsedNs(Stopwatch sw) =>
        sw.ElapsedTicks * (1_000_000_000L / Stopwatch.Frequency);

    static void BenchArcLocals(int iters)
    {
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            string a = "n" + i.ToString();
            string b = a;
            string c = b;
            acc += c.Length;
        }
        sw.Stop();
        Report("arc_locals", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchStringConcat(int iters)
    {
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            string s = "hello" + i.ToString() + "world";
            acc += s.Length;
        }
        sw.Stop();
        Report("string_concat", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchStringEq(int iters)
    {
        string a = "abcdefghijklmnopqrstuvwxyz0123456789";
        string b = "abcdefghijklmnopqrstuvwxyz0123456789";
        string c = "abcdefghijklmnopqrstuvwxyz0123456780";
        var sw = Stopwatch.StartNew();
        int hits = 0;
        for (int i = 0; i < iters; i++)
        {
            if (a == b) hits++;
            if (a == c) hits--;
        }
        sw.Stop();
        Report("string_eq", ElapsedNs(sw), iters);
        Sink = hits;
    }

    // Linear UTF-16 scan — fair counterpart to Dream's chars() iterator (not O(n²) char_at).
    static void BenchCharScan(int iters)
    {
        string s = "The quick brown fox jumps over the lazy dog. 0123456789";
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            foreach (char ch in s)
                acc += (int)ch;
        }
        sw.Stop();
        Report("char_scan", ElapsedNs(sw), iters);
        Sink = acc;
    }

    // Raw code-unit / byte walk — fair ASCII counterpart to Dream byte_scan.
    static void BenchByteScan(int iters)
    {
        string s = "The quick brown fox jumps over the lazy dog. 0123456789";
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            for (int j = 0; j < s.Length; j++)
                acc += (int)s[j];
        }
        sw.Stop();
        Report("byte_scan", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchSubstring(int iters)
    {
        string s = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            // Dream substring(5, 40) is start/end → length 35.
            string sub = s.Substring(5, 35);
            acc += sub.Length;
        }
        sw.Stop();
        Report("substring", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchListPush(int iters)
    {
        var sw = Stopwatch.StartNew();
        var list = new List<int>(iters);
        for (int i = 0; i < iters; i++)
            list.Add(i);
        sw.Stop();
        Report("list_push", ElapsedNs(sw), iters);
        Sink = list.Count;
    }

    static void BenchListInsertMid(int iters)
    {
        int n = 256;
        int rounds = Math.Max(1, iters / n);
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < rounds; r++)
        {
            var list = new List<int>(n);
            for (int i = 0; i < n; i++)
                list.Insert(list.Count / 2, i);
        }
        sw.Stop();
        Report("list_insert_mid", ElapsedNs(sw), rounds * n);
    }

    static void BenchMapGetSet(int iters)
    {
        var map = new Dictionary<int, int>(iters);
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iters; i++)
            map[i] = i * 3;
        int acc = 0;
        for (int i = 0; i < iters; i++)
            acc += map.TryGetValue(i, out int val) ? val : 0;
        sw.Stop();
        Report("map_get_set", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchMapClearReuse(int iters)
    {
        var map = new Dictionary<int, int>(1024);
        int rounds = 64;
        int per = Math.Max(1, iters / rounds);
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < rounds; r++)
        {
            for (int i = 0; i < per; i++)
                map[i] = i;
            map.Clear();
        }
        sw.Stop();
        Report("map_clear_reuse", ElapsedNs(sw), rounds * per);
        Sink = map.Count;
    }

    static void BenchAllocChurn(int iters)
    {
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            int[] buf = new int[64];
            buf[0] = i;
            acc += buf[0];
        }
        sw.Stop();
        Report("alloc_churn", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchListClearReuse(int iters)
    {
        var buf = new List<int>(64);
        int rounds = 64;
        int per = Math.Max(1, iters / rounds);
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < rounds; r++)
        {
            for (int i = 0; i < per; i++)
                buf.Add(i);
            buf.Clear();
        }
        sw.Stop();
        Report("list_clear_reuse", ElapsedNs(sw), rounds * per);
        Sink = buf.Capacity;
    }

    static void BenchScratchArena(int iters)
    {
        var arena = new ScratchArena<int>(1 << 14);
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            int start = arena.Bump(32);
            arena.SetAt(start, i);
            acc += arena.At(start);
            arena.Reset();
        }
        sw.Stop();
        Report("scratch_arena", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchRegexFind(int iters)
    {
        // Same as Dream: not bare \d+ (Dream has a digit-run fast path for that).
        var re = new Regex(@"[a-z]+\d+", RegexOptions.Compiled);
        string hay = "abc123def456ghi789xyz";
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var matches = re.Matches(hay);
            foreach (Match m in matches)
                acc += m.Length;
        }
        sw.Stop();
        Report("regex_find", ElapsedNs(sw), iters);
        Sink = acc;
    }

    sealed class BenchAddress
    {
        public string city { get; set; } = "";
        public string zip { get; set; } = "";
    }

    sealed class BenchUser
    {
        public string name { get; set; } = "";
        public int age { get; set; }
        public bool active { get; set; }
        public BenchAddress address { get; set; } = new();
        public string[] tags { get; set; } = [];
        public int[] scores { get; set; } = [];
    }

    static BenchUser MakeBenchUser() => new()
    {
        name = "Ada",
        age = 36,
        active = true,
        address = new BenchAddress { city = "London", zip = "NW1" },
        tags = ["dev", "math"],
        scores = [10, 20, 30],
    };

    static void BenchJsonSerialize(int iters)
    {
        var u = MakeBenchUser();
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            string text = JsonSerializer.Serialize(u);
            acc += text.Length;
        }
        sw.Stop();
        Report("json_serialize", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchJsonDeserialize(int iters)
    {
        string text = JsonSerializer.Serialize(MakeBenchUser());
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var back = JsonSerializer.Deserialize<BenchUser>(text)!;
            acc += back.age + back.scores[2];
        }
        sw.Stop();
        Report("json_deserialize", ElapsedNs(sw), iters);
        Sink = acc;
    }

    static void BenchArrAdd(int iters)
    {
        int n = 256;
        var a = new float[n];
        var b = new float[n];
        var c = new float[n];
        var ai = new int[n];
        var bi = new int[n];
        var ci = new int[n];
        for (int k = 0; k < n; k++)
        {
            a[k] = k;
            b[k] = k + 1;
            ai[k] = k;
            bi[k] = k + 1;
        }
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < iters; r++)
        {
            for (int i = 0; i < n; i++)
                c[i] = a[i] + b[i];
            for (int j = 0; j < n; j++)
                ci[j] = ai[j] + bi[j];
        }
        sw.Stop();
        Report("arr_add", ElapsedNs(sw), iters);
        int acc = 0;
        for (int i = 0; i < n; i++)
            acc += (int)c[i] + ci[i];
        Sink = acc;
    }

    static void BenchVecAdd(int iters)
    {
        int n = 256;
        var a = new float[n];
        var b = new float[n];
        var c = new float[n];
        for (int k = 0; k < n; k++)
        {
            a[k] = k;
            b[k] = k + 1;
        }
        int lanes = Vector<float>.Count;
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < iters; r++)
        {
            int i = 0;
            for (; i + lanes <= n; i += lanes)
            {
                var va = new Vector<float>(a, i);
                var vb = new Vector<float>(b, i);
                (va + vb).CopyTo(c, i);
            }
            for (; i < n; i++)
                c[i] = a[i] + b[i];
        }
        sw.Stop();
        Report("vec_add", ElapsedNs(sw), iters);
        int acc = 0;
        for (int i = 0; i < n; i++)
            acc += (int)c[i];
        Sink = acc;
    }

    static void BenchStringBuilder(int iters)
    {
        string chunk = "abcdefghijklmnopqrstuvwxyz";
        var sw = Stopwatch.StartNew();
        var sb = new StringBuilder(iters * chunk.Length);
        for (int i = 0; i < iters; i++)
            sb.Append(chunk);
        string built = sb.ToString();
        sw.Stop();
        Report("string_builder", ElapsedNs(sw), iters);
        Sink = built.Length;
    }

    static void RunSuite()
    {
        int scale = 20000;
        BenchArcLocals(scale);
        BenchStringConcat(scale);
        BenchStringEq(scale * 5);
        BenchCharScan(scale / 10);
        BenchByteScan(scale / 10);
        BenchSubstring(scale);
        BenchListPush(scale);
        BenchListInsertMid(scale);
        BenchMapGetSet(scale);
        BenchMapClearReuse(scale);
        BenchListClearReuse(scale);
        BenchAllocChurn(scale);
        BenchScratchArena(scale);
        BenchRegexFind(scale / 10);
        BenchStringBuilder(scale / 5);
        BenchJsonSerialize(scale / 10);
        BenchJsonDeserialize(scale / 10);
        BenchArrAdd(scale / 10);
        BenchVecAdd(scale / 10);
    }

    static void LoadDreamScores(string path)
    {
        foreach (string line in File.ReadLines(path))
        {
            // bench <name> ns_total=… iters=… ns_per_op=<n>
            if (!line.StartsWith("bench ", StringComparison.Ordinal)) continue;
            int nsIdx = line.LastIndexOf("ns_per_op=", StringComparison.Ordinal);
            if (nsIdx < 0) continue;
            string rest = line["bench ".Length..];
            int sp = rest.IndexOf(' ');
            if (sp <= 0) continue;
            string name = rest[..sp];
            string num = line[(nsIdx + "ns_per_op=".Length)..].Trim();
            if (long.TryParse(num, out long v))
                DreamScores[name] = v;
        }
    }

    public static int Main(string[] args)
    {
        string? scores = Environment.GetEnvironmentVariable("DREAM_SCORES");
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--dream-scores" && i + 1 < args.Length)
                scores = args[++i];
        }
        if (!string.IsNullOrEmpty(scores) && File.Exists(scores))
            LoadDreamScores(scores);

        IsWarmup = true;
        RunSuite();
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();

        IsWarmup = false;
        RunSuite();
        return Sink == int.MinValue ? 1 : 0;
    }
}
