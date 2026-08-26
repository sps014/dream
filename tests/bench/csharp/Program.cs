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
        // Fractional ns/op: integer division truncates sub-nanosecond benches to 0-1.
        double csharpNs = (double)elapsedNanos / iters;
        Console.WriteLine($"bench {name} ns_per_op={csharpNs.ToString("F1", System.Globalization.CultureInfo.InvariantCulture)}");
        if (DreamScores.TryGetValue(name, out long dreamNs) && dreamNs > 0)
        {
            double ratio = csharpNs / dreamNs;
            string cmp = ratio > 1.0
                ? $"C# is {ratio:F1}x slower"
                : $"C# is {(1.0 / ratio):F1}x faster";
            Console.Error.WriteLine($"  compare {name,-18} C#={csharpNs,6:F1} Dream={dreamNs,6} | {cmp}");
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
        // Mirror of Dream: comparand rotates per iteration so the JIT cannot hoist the
        // comparison out of the loop. variants[0] shares content with base (interned).
        string @base = "abcdefghijklmnopqrstuvwxyz0123456789";
        string[] variants =
        [
            "abcdefghijklmnopqrstuvwxyz0123456789",
            "abcdefghijklmnopqrstuvwxyz0123456780",
            "abcdefghijKlmnopqrstuvwxyz0123456789",
            "abcdefghijklmnopqrstuvwxyz012345678",
        ];
        var sw = Stopwatch.StartNew();
        int hits = 0;
        for (int i = 0; i < iters; i++)
        {
            if (@base == variants[i & 3]) hits++;
            else hits--;
            Sink += hits;
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

    // =====================================================================
    // Compute kernels (mirrors of the Dream additions).
    // =====================================================================

    sealed class Body
    {
        public double X, Y, Z, Vx, Vy, Vz, Mass;
        public Body(double x, double y, double z, double vx, double vy, double vz, double mass)
        { X = x; Y = y; Z = z; Vx = vx; Vy = vy; Vz = vz; Mass = mass; }
    }

    static void BenchNbody(int iters)
    {
        var bodies = new[]
        {
            new Body(0.0, 0.0, 0.0, 0.01, 0.0, 0.0, 1.0),
            new Body(1.0, 0.5, 0.2, 0.0, 0.02, 0.0, 0.5),
            new Body(-1.2, 0.3, -0.4, 0.01, 0.0, 0.03, 0.25),
            new Body(0.4, -0.9, 0.8, -0.02, 0.01, 0.0, 0.125),
            new Body(0.9, 0.9, -0.7, 0.0, -0.01, 0.02, 0.0625),
        };
        int n = bodies.Length;
        const double dt = 0.01;
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iters; i++)
        {
            for (int a = 0; a < n; a++)
            {
                var bi = bodies[a];
                for (int b = a + 1; b < n; b++)
                {
                    var bj = bodies[b];
                    double dx = bj.X - bi.X, dy = bj.Y - bi.Y, dz = bj.Z - bi.Z;
                    double d2 = dx * dx + dy * dy + dz * dz + 1e-9;
                    double inv = 1.0 / Math.Sqrt(d2);
                    double f = bj.Mass * bi.Mass * inv * inv * inv * dt;
                    bi.Vx += dx * f; bi.Vy += dy * f; bi.Vz += dz * f;
                    bj.Vx -= dx * f; bj.Vy -= dy * f; bj.Vz -= dz * f;
                }
            }
            for (int k = 0; k < n; k++)
            {
                var bk = bodies[k];
                bk.X += bk.Vx * dt; bk.Y += bk.Vy * dt; bk.Z += bk.Vz * dt;
            }
        }
        sw.Stop();
        Report("nbody", ElapsedNs(sw), iters);
        Sink = (int)(bodies[0].X * 1000.0);
    }

    static void BenchMandelbrot(int iters)
    {
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
        {
            for (int row = 0; row < 48; row++)
            {
                double ci = row / 24.0 - 1.0;
                for (int col = 0; col < 64; col++)
                {
                    double cr = col / 32.0 - 1.5;
                    double zr = 0, zi = 0;
                    int k = 0;
                    bool escaped = false;
                    while (k < 30 && !escaped)
                    {
                        double zr2 = zr * zr - zi * zi + cr;
                        zi = 2.0 * zr * zi + ci;
                        zr = zr2;
                        if (zr * zr + zi * zi > 4.0) escaped = true;
                        k++;
                    }
                    acc += k;
                    Sink = (int)acc;
                }
            }
        }
        sw.Stop();
        Report("mandelbrot", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    const int MatN = 64;

    static void BenchMatmul(int iters)
    {
        int n = MatN;
        var a = new double[n * n];
        var b = new double[n * n];
        var c = new double[n * n];
        for (int t = 0; t < n * n; t++)
        {
            a[t] = t % 13;
            b[t] = (t * 7) % 11;
            c[t] = 0.0;
        }
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < iters; r++)
        {
            for (int i = 0; i < n; i++)
            {
                for (int k = 0; k < n; k++)
                {
                    double aik = a[i * n + k];
                    for (int j = 0; j < n; j++)
                        c[i * n + j] += aik * b[k * n + j];
                }
            }
        }
        sw.Stop();
        Report("matmul_64", ElapsedNs(sw), iters);
        Sink = (int)c[0];
    }

    static void QsortRange(int[] a, int lo, int hi)
    {
        if (lo >= hi) return;
        int p = a[(lo + hi) / 2];
        int i = lo, j = hi;
        while (i <= j)
        {
            while (a[i] < p) i++;
            while (a[j] > p) j--;
            if (i <= j)
            {
                (a[i], a[j]) = (a[j], a[i]);
                i++; j--;
            }
        }
        QsortRange(a, lo, j);
        QsortRange(a, i, hi);
    }

    static void BenchQuicksort(int iters)
    {
        int n = 512;
        var a = new int[n];
        int seed = 123456789;
        var sw = Stopwatch.StartNew();
        for (int r = 0; r < iters; r++)
        {
            for (int t = 0; t < n; t++)
            {
                seed = unchecked(seed * 1103515245 + 12345);
                a[t] = (seed >> 16) & 1023;
            }
            QsortRange(a, 0, n - 1);
        }
        sw.Stop();
        Report("quicksort", ElapsedNs(sw), iters);
        Sink = a[0];
    }

    static void BenchSieve(int iters)
    {
        int n = 4096;
        var flags = new int[n];
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int r = 0; r < iters; r++)
        {
            Array.Fill(flags, 1);
            for (int i = 2; (long)i * i < n; i++)
            {
                if (flags[i] == 1)
                {
                    for (int m = i * i; m < n; m += i) flags[m] = 0;
                }
            }
            for (int p = 2; p < n; p++) acc += flags[p];
            Sink = (int)acc;
        }
        sw.Stop();
        Report("sieve", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    // =====================================================================
    // Call / dispatch / recursion.
    // =====================================================================

    static int Fib(int n) => n < 2 ? n : Fib(n - 1) + Fib(n - 2);

    static void BenchFibRec(int iters)
    {
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
            acc += Fib(20 + (i & 1));
        sw.Stop();
        Report("fib_rec", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    interface BenchOp { int Apply(int x); }

    sealed class AddOp : BenchOp { public int Apply(int x) => x + 3; }
    sealed class MulOp : BenchOp { public int Apply(int x) => x * 3; }
    sealed class SubOp : BenchOp { public int Apply(int x) => x - 3; }

    static void BenchIfaceDispatch(int iters)
    {
        BenchOp a = new AddOp(), b = new MulOp(), c = new SubOp();
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
            acc += a.Apply(i) + b.Apply(i) + c.Apply(i);
        sw.Stop();
        Report("iface_dispatch", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    // =====================================================================
    // ARC / allocator reality at scale.
    // =====================================================================

    sealed class TreeNode
    {
        public TreeNode? Left, Right;
        public TreeNode(TreeNode? left, TreeNode? right) { Left = left; Right = right; }
    }

    static TreeNode? MakeTree(int depth) =>
        depth <= 0 ? null : new TreeNode(MakeTree(depth - 1), MakeTree(depth - 1));

    static void BenchBinaryTrees(int iters)
    {
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var root = MakeTree(12);
            if (root != null) acc++;
            else acc--;
        }
        sw.Stop();
        Report("binary_trees", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    sealed class LinkNode
    {
        public int Value;
        public LinkNode? Next;
        public LinkNode(int value, LinkNode? next) { Value = value; Next = next; }
    }

    static void BenchLinkedWalk(int iters)
    {
        LinkNode? head = null;
        for (int i = 0; i < 1024; i++)
            head = new LinkNode(i, head);
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var curr = head;
            while (curr != null)
            {
                acc += curr.Value;
                curr = curr.Next;
            }
        }
        sw.Stop();
        Report("linked_walk", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    // =====================================================================
    // Collections / strings / enums.
    // =====================================================================

    static void BenchWordcount(int iters)
    {
        string text = "the quick brown fox jumps over the lazy dog the end the fox and the dog run with the quick fox";
        string[] words = text.Split(' ');
        var map = new Dictionary<string, int>(256);
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iters; i++)
        {
            foreach (var w in words)
                map[w] = map.TryGetValue(w, out int v) ? v + 1 : 1;
        }
        sw.Stop();
        Report("wordcount", ElapsedNs(sw), iters * words.Length);
        Sink = map.GetValueOrDefault("the");
    }

    static void BenchParseInts(int iters)
    {
        string src = "1234567890";
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
        {
            int v = 0;
            for (int j = 0; j < src.Length; j++)
                v = v * 10 + (src[j] - '0');
            acc += v;
        }
        sw.Stop();
        Report("parse_ints", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    static void BenchSumOptions(int iters)
    {
        int? some = 7, none = null;
        var sw = Stopwatch.StartNew();
        long acc = 0;
        for (int i = 0; i < iters; i++)
        {
            if (some is int sv) acc += sv; else acc--;
            if (none is int nv) acc += nv; else acc++;
        }
        sw.Stop();
        Report("sum_options", ElapsedNs(sw), iters);
        Sink = (int)acc;
    }

    static void RunSuite()
    {
        int scale = 20000;
        // compute kernels
        BenchNbody(scale / 10);
        BenchMandelbrot(scale / 100);
        BenchMatmul(scale / 50);
        BenchQuicksort(scale / 20);
        BenchSieve(scale / 50);
        // call / dispatch / recursion
        BenchFibRec(scale / 20);
        BenchIfaceDispatch(scale / 5);
        // ARC / allocator reality
        BenchBinaryTrees(scale / 200);
        BenchLinkedWalk(scale / 5);
        // collections / strings / enums
        BenchWordcount(scale / 5);
        BenchParseInts(scale);
        BenchSumOptions(scale);
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
