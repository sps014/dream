using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace DreamBench;

/// <summary>
/// 1:1 C# port of tests/bench/macro/macrobenches.dream. Inputs come from the same LCG streams, and
/// every checksum is built from integers so both runtimes must agree exactly.
/// </summary>
public static partial class Program
{
    static bool IsWarmup = true;
    static int Sink;

    static void Report(string name, long elapsedNanos, int iters, int checksum)
    {
        if (IsWarmup) return;
        Console.WriteLine($"bench {name} ns_total={elapsedNanos} iters={iters} checksum={checksum}");
    }

    static long ElapsedNs(Stopwatch sw) =>
        (long)((decimal)sw.ElapsedTicks * 1_000_000_000L / Stopwatch.Frequency);

    static int SeedBit() =>
        int.Parse(Environment.GetEnvironmentVariable("DREAM_BENCH_SEED") ?? "1") & 1;

    sealed class Lcg
    {
        int s;
        public Lcg(int seed) => s = seed;
        public int Next(int bound)
        {
            s = unchecked(s * 1103515245 + 12345);
            return ((s >> 8) & 16777215) % bound;
        }
    }

    // ---- json_service ----------------------------------------------------------------------

    public sealed class LineItem
    {
        [JsonPropertyName("sku")] public string Sku { get; set; } = "";
        [JsonPropertyName("qty")] public int Qty { get; set; }
        [JsonPropertyName("price")] public double Price { get; set; }
    }

    public sealed class Order
    {
        [JsonPropertyName("id")] public int Id { get; set; }
        [JsonPropertyName("customer")] public string Customer { get; set; } = "";
        [JsonPropertyName("tags")] public List<string> Tags { get; set; } = new();
        [JsonPropertyName("items")] public List<LineItem> Items { get; set; } = new();
    }

    public sealed class Receipt
    {
        [JsonPropertyName("order")] public int OrderId { get; set; }
        [JsonPropertyName("customer")] public string Customer { get; set; } = "";
        [JsonPropertyName("total")] public double Total { get; set; }
        [JsonPropertyName("units")] public int Units { get; set; }
        [JsonPropertyName("skus")] public List<string> Skus { get; set; } = new();
    }

    [JsonSerializable(typeof(Order))]
    [JsonSerializable(typeof(Receipt))]
    internal partial class MacroJson : JsonSerializerContext { }

    static string MakeOrderText(Lcg rng, int id)
    {
        var tags = new List<string> { "priority-" + rng.Next(4), "region-" + rng.Next(16) };
        var items = new List<LineItem>();
        int n = 8 + rng.Next(24);
        for (int i = 0; i < n; i++)
        {
            string sku = "SKU-" + rng.Next(5000);
            int qty = 1 + rng.Next(9);
            double price = rng.Next(100000) / 100.0;
            items.Add(new LineItem { Sku = sku, Qty = qty, Price = price });
        }
        string customer = "customer-" + rng.Next(100000);
        return JsonSerializer.Serialize(new Order { Id = id, Customer = customer, Tags = tags, Items = items }, MacroJson.Default.Order);
    }

    static void BenchJsonService(int iters)
    {
        var rng = new Lcg(17 + SeedBit());
        var docs = new List<string>();
        for (int d = 0; d < 64; d++) docs.Add(MakeOrderText(rng, d));
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var order = JsonSerializer.Deserialize(docs[i & 63], MacroJson.Default.Order) ?? new Order();
            double total = 0;
            int units = 0;
            var skus = new List<string>();
            foreach (var it in order.Items)
            {
                total += it.Price * it.Qty;
                units += it.Qty;
                if (it.Qty > 5) skus.Add(it.Sku);
            }
            string outText = JsonSerializer.Serialize(new Receipt { OrderId = order.Id, Customer = order.Customer, Total = total, Units = units, Skus = skus }, MacroJson.Default.Receipt);
            Sink += outText.Length;
            acc = unchecked(acc + order.Id + units * 3 + skus.Count * 7 + order.Tags.Count);
        }
        sw.Stop();
        Sink += acc;
        Report("json_service", ElapsedNs(sw), iters, acc);
    }

    // ---- log_processor ---------------------------------------------------------------------

    static string MakeLog(Lcg rng, int lines)
    {
        string[] levels = { "INFO", "WARN", "ERROR", "DEBUG" };
        string[] paths = { "/api/users", "/api/orders", "/static/app.js", "/health", "/api/search", "/login" };
        var sb = new StringBuilder(lines * 64);
        for (int i = 0; i < lines; i++)
        {
            sb.Append("2026-10-09T12:");
            sb.Append(10 + rng.Next(50));
            sb.Append(' ');
            sb.Append(levels[rng.Next(4)]);
            sb.Append(' ');
            sb.Append(paths[rng.Next(6)]);
            sb.Append(" status=");
            int status = 200 + rng.Next(4) * 100;
            status += rng.Next(3);
            sb.Append(status);
            sb.Append(" ms=");
            sb.Append(rng.Next(900));
            sb.Append(" user=u");
            sb.Append(rng.Next(500));
            sb.Append('\n');
        }
        return sb.ToString();
    }

    static void BenchLogProcessor(int iters)
    {
        var rng = new Lcg(29 + SeedBit());
        string text = MakeLog(rng, 2000);
        var statusRe = new Regex(@"status=(5\d\d)");
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var byPath = new Dictionary<string, int>();
            int errors = 0;
            foreach (var line in text.Split('\n'))
            {
                if (line.Length == 0) continue;
                var parts = line.Split(' ');
                string key = parts[1] + " " + parts[2];
                byPath[key] = byPath.GetValueOrDefault(key, 0) + 1;
                if (statusRe.IsMatch(line)) errors++;
            }
            acc = unchecked(acc + byPath.Count * 1000 + errors);
        }
        sw.Stop();
        Sink += acc;
        Report("log_processor", ElapsedNs(sw), iters, acc);
    }

    // ---- graph_paths -----------------------------------------------------------------------

    sealed class Edge
    {
        public readonly int To, W;
        public Edge(int to, int w) { To = to; W = w; }
    }

    sealed class Vertex
    {
        public readonly int Id;
        public readonly List<Edge> Out = new();
        public Vertex(int id) => Id = id;
    }

    static List<Vertex> MakeGraph(Lcg rng, int n, int degree)
    {
        var g = new List<Vertex>();
        for (int i = 0; i < n; i++) g.Add(new Vertex(i));
        for (int i = 0; i < n; i++)
        {
            for (int k = 0; k < degree; k++)
            {
                int to = rng.Next(n);
                int w = 1 + rng.Next(100);
                g[i].Out.Add(new Edge(to, w));
            }
            g[(i + 1) % n].Out.Add(new Edge(i, 50));
        }
        return g;
    }

    static int ShortestSum(List<Vertex> g, int src)
    {
        int n = g.Count;
        var dist = new int[n];
        Array.Fill(dist, int.MaxValue);
        var pq = new PriorityQueue<(int node, int dist), int>();
        dist[src] = 0;
        pq.Enqueue((src, 0), 0);
        while (pq.TryDequeue(out var v, out _))
        {
            if (v.dist > dist[v.node]) continue;
            foreach (var edge in g[v.node].Out)
            {
                int nd = v.dist + edge.W;
                if (nd < dist[edge.To])
                {
                    dist[edge.To] = nd;
                    pq.Enqueue((edge.To, nd), nd);
                }
            }
        }
        int total = 0;
        foreach (int d in dist) if (d != int.MaxValue) total = unchecked(total + d);
        return total;
    }

    static void BenchGraphPaths(int iters)
    {
        var rng = new Lcg(41 + SeedBit());
        var g = MakeGraph(rng, 2000, 6);
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++) acc = unchecked(acc + ShortestSum(g, (i * 37) % 2000));
        sw.Stop();
        Sink += acc;
        Report("graph_paths", ElapsedNs(sw), iters, acc);
    }

    // ---- async_fanout ----------------------------------------------------------------------

    static async Task<int> LeafJob(int i)
    {
        if ((i & 7) == 0) await Task.Yield();
        return (i * 31) & 1023;
    }

    static async Task<int> MidJob(int b)
    {
        var parts = await Task.WhenAll(LeafJob(b), LeafJob(b + 1), LeafJob(b + 2), LeafJob(b + 3));
        return parts[0] + parts[1] + parts[2] + parts[3];
    }

    static async Task BenchAsyncFanout(int iters)
    {
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var batch = new Task<int>[64];
            for (int k = 0; k < 64; k++) batch[k] = MidJob(i + k * 4);
            var results = await Task.WhenAll(batch);
            foreach (int r in results) acc = unchecked(acc + r);
            acc = unchecked(acc + await await Task.WhenAny(LeafJob(i), LeafJob(i + 1024)));
        }
        sw.Stop();
        Sink += acc;
        Report("async_fanout", ElapsedNs(sw), iters, acc);
    }

    // ---- task_parallel ---------------------------------------------------------------------

    static int ChunkWork(int seed)
    {
        int h = seed;
        int acc = 0;
        for (int i = 0; i < 20000; i++)
        {
            h = unchecked(h * 1103515245 + 12345);
            acc += (h >> 16) & 255;
        }
        return acc;
    }

    static async Task BenchTaskParallel(int iters)
    {
        var seeds = new int[16];
        var sw = Stopwatch.StartNew();
        int acc = 0;
        for (int i = 0; i < iters; i++)
        {
            for (int k = 0; k < 16; k++) seeds[k] = i * 16 + k + SeedBit();
            var tasks = new Task<int>[16];
            for (int k = 0; k < 16; k++)
            {
                int s = seeds[k];
                tasks[k] = Task.Run(() => ChunkWork(s));
            }
            foreach (int r in await Task.WhenAll(tasks)) acc = unchecked(acc + r);
        }
        sw.Stop();
        Sink += acc;
        Report("task_parallel", ElapsedNs(sw), iters, acc);
    }

    static async Task RunSuite(int scale)
    {
        BenchJsonService(scale * 20);
        BenchLogProcessor(scale / 10);
        BenchGraphPaths(scale / 4);
        await BenchAsyncFanout(scale * 2);
        await BenchTaskParallel(scale / 2);
    }

    public static async Task Main()
    {
        int scale = 100;
        IsWarmup = true;
        await RunSuite(scale);
        IsWarmup = false;
        int passes = Math.Clamp(int.Parse(Environment.GetEnvironmentVariable("DREAM_BENCH_PASSES") ?? "1"), 1, 30);
        for (int p = 0; p < passes; p++) await RunSuite(scale);
        Console.WriteLine($"sink {Sink}");
    }
}
