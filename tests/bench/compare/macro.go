// Go counterpart of tests/bench/macro/macrobenches.dream.
// Same scale and 32-bit LCG. Goroutines stand in for Dream tasks and async.
package main

import (
	"encoding/json"
	"os"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

const scale = 100

var measuring = true
var sink int32

//go:noinline
func sinkInt(x int32) int32 {
	sink += x
	return x
}

func report(name string, start time.Time, iters int, checksum int32) {
	if !measuring {
		return
	}
	ns := time.Since(start).Nanoseconds()
	per := ns / int64(iters)
	os.Stdout.WriteString("bench " + name + " ns_total=" + strconv.FormatInt(ns, 10) +
		" iters=" + strconv.Itoa(iters) + " ns_per_op=" + strconv.FormatInt(per, 10) +
		" checksum=" + strconv.FormatInt(int64(checksum), 10) + "\n")
}

func seedBit() int32 {
	v := os.Getenv("DREAM_BENCH_SEED")
	if v == "" {
		v = "1"
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		n = 1
	}
	return int32(n) & 1
}

type lcg struct{ s int32 }

func (r *lcg) next(bound int32) int32 {
	r.s = r.s*1103515245 + 12345
	return ((r.s >> 8) & 16777215) % bound
}

type lineItem struct {
	Sku   string  `json:"sku"`
	Qty   int32   `json:"qty"`
	Price float64 `json:"price"`
}
type order struct {
	ID       int32      `json:"id"`
	Customer string     `json:"customer"`
	Tags     []string   `json:"tags"`
	Items    []lineItem `json:"items"`
}
type receipt struct {
	Order    int32    `json:"order"`
	Customer string   `json:"customer"`
	Total    float64  `json:"total"`
	Units    int32    `json:"units"`
	Skus     []string `json:"skus"`
}

func makeOrder(rng *lcg, id int32) string {
	tags := []string{
		"priority-" + strconv.Itoa(int(rng.next(4))),
		"region-" + strconv.Itoa(int(rng.next(16))),
	}
	n := 8 + rng.next(24)
	items := make([]lineItem, 0, n)
	for i := int32(0); i < n; i++ {
		items = append(items, lineItem{
			Sku:   "SKU-" + strconv.Itoa(int(rng.next(5000))),
			Qty:   1 + rng.next(9),
			Price: float64(rng.next(100000)) / 100.0,
		})
	}
	text, err := json.Marshal(order{id, "customer-" + strconv.Itoa(int(rng.next(100000))), tags, items})
	if err != nil {
		panic(err)
	}
	return string(text)
}

func benchJSON(iters int) {
	rng := lcg{17 + seedBit()}
	docs := make([]string, 64)
	for d := 0; d < 64; d++ {
		docs[d] = makeOrder(&rng, int32(d))
	}
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		var ord order
		if err := json.Unmarshal([]byte(docs[i&63]), &ord); err != nil {
			panic(err)
		}
		total := 0.0
		units := int32(0)
		var skus []string
		for _, it := range ord.Items {
			total += it.Price * float64(it.Qty)
			units += it.Qty
			if it.Qty > 5 {
				skus = append(skus, it.Sku)
			}
		}
		out, err := json.Marshal(receipt{ord.ID, ord.Customer, total, units, skus})
		if err != nil {
			panic(err)
		}
		sinkInt(int32(len(out)))
		acc += ord.ID + units*3 + int32(len(skus))*7 + int32(len(ord.Tags))
	}
	sinkInt(acc)
	report("json_service", start, iters, acc)
}

func makeLog(rng *lcg, lines int) string {
	levels := []string{"INFO", "WARN", "ERROR", "DEBUG"}
	paths := []string{"/api/users", "/api/orders", "/static/app.js", "/health", "/api/search", "/login"}
	var sb strings.Builder
	sb.Grow(lines * 64)
	for i := 0; i < lines; i++ {
		sb.WriteString("2026-10-09T12:")
		sb.WriteString(strconv.Itoa(int(10 + rng.next(50))))
		sb.WriteByte(' ')
		sb.WriteString(levels[rng.next(4)])
		sb.WriteByte(' ')
		sb.WriteString(paths[rng.next(6)])
		sb.WriteString(" status=")
		sb.WriteString(strconv.Itoa(int(200 + rng.next(4)*100 + rng.next(3))))
		sb.WriteString(" ms=")
		sb.WriteString(strconv.Itoa(int(rng.next(900))))
		sb.WriteString(" user=u")
		sb.WriteString(strconv.Itoa(int(rng.next(500))))
		sb.WriteByte('\n')
	}
	return sb.String()
}

func benchLog(iters int) {
	rng := lcg{29 + seedBit()}
	text := makeLog(&rng, 2000)
	statusRe := regexp.MustCompile(`status=(5\d\d)`)
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		byPath := map[string]int32{}
		errors := int32(0)
		for _, line := range strings.Split(text, "\n") {
			if len(line) == 0 {
				continue
			}
			parts := strings.Split(line, " ")
			key := parts[1] + " " + parts[2]
			byPath[key] = byPath[key] + 1
			if statusRe.MatchString(line) {
				errors++
			}
		}
		acc += int32(len(byPath))*1000 + errors
	}
	sinkInt(acc)
	report("log_processor", start, iters, acc)
}

type edge struct {
	to int32
	w  int32
}
type vertex struct {
	id  int32
	out []edge
}
type visit struct {
	node int32
	dist int32
}

func makeGraph(rng *lcg, n, degree int) []vertex {
	g := make([]vertex, n)
	for i := 0; i < n; i++ {
		g[i].id = int32(i)
	}
	for i := 0; i < n; i++ {
		for k := 0; k < degree; k++ {
			g[i].out = append(g[i].out, edge{rng.next(int32(n)), 1 + rng.next(100)})
		}
		g[(i+1)%n].out = append(g[(i+1)%n].out, edge{int32(i), 50})
	}
	return g
}

type heap4 struct{ items []visit }

func (h *heap4) less(a, b int) bool { return h.items[a].dist < h.items[b].dist }

func (h *heap4) push(v visit) {
	h.items = append(h.items, v)
	i := len(h.items) - 1
	for i > 0 {
		p := (i - 1) / 4
		if h.less(i, p) {
			h.items[i], h.items[p] = h.items[p], h.items[i]
			i = p
		} else {
			break
		}
	}
}

func (h *heap4) pop() (visit, bool) {
	n := len(h.items)
	if n == 0 {
		return visit{}, false
	}
	top := h.items[0]
	last := h.items[n-1]
	h.items = h.items[:n-1]
	if n == 1 {
		return top, true
	}
	h.items[0] = last
	i := 0
	for {
		first := i*4 + 1
		if first >= len(h.items) {
			break
		}
		best := first
		for off := 1; off < 4 && first+off < len(h.items); off++ {
			if h.items[first+off].dist < h.items[best].dist {
				best = first + off
			}
		}
		if h.items[best].dist < h.items[i].dist {
			h.items[i], h.items[best] = h.items[best], h.items[i]
			i = best
		} else {
			break
		}
	}
	return top, true
}

type heap4ptr struct{ items []*visit }

func (h *heap4ptr) push(v *visit) {
	h.items = append(h.items, v)
	i := len(h.items) - 1
	for i > 0 {
		p := (i - 1) / 4
		if h.items[i].dist < h.items[p].dist {
			h.items[i], h.items[p] = h.items[p], h.items[i]
			i = p
		} else {
			break
		}
	}
}

func (h *heap4ptr) pop() (*visit, bool) {
	n := len(h.items)
	if n == 0 {
		return nil, false
	}
	top := h.items[0]
	last := h.items[n-1]
	h.items = h.items[:n-1]
	if n == 1 {
		return top, true
	}
	h.items[0] = last
	i := 0
	for {
		first := i*4 + 1
		if first >= len(h.items) {
			break
		}
		best := first
		for off := 1; off < 4 && first+off < len(h.items); off++ {
			if h.items[first+off].dist < h.items[best].dist {
				best = first + off
			}
		}
		if h.items[best].dist < h.items[i].dist {
			h.items[i], h.items[best] = h.items[best], h.items[i]
			i = best
		} else {
			break
		}
	}
	return top, true
}

func shortestClass(g []vertex, src int32) int32 {
	dist := make([]int32, len(g))
	for i := range dist {
		dist[i] = 2147483647
	}
	var pq heap4ptr
	dist[src] = 0
	pq.push(&visit{src, 0})
	for v, ok := pq.pop(); ok; v, ok = pq.pop() {
		if v.dist <= dist[v.node] {
			for _, e := range g[v.node].out {
				nd := v.dist + e.w
				if nd < dist[e.to] {
					dist[e.to] = nd
					pq.push(&visit{e.to, nd})
				}
			}
		}
	}
	total := int32(0)
	for _, d := range dist {
		if d != 2147483647 {
			total += d
		}
	}
	return total
}

func shortestValue(g []vertex, src int32) int32 {
	dist := make([]int32, len(g))
	for i := range dist {
		dist[i] = 2147483647
	}
	var pq heap4
	dist[src] = 0
	pq.push(visit{src, 0})
	for v, ok := pq.pop(); ok; v, ok = pq.pop() {
		if v.dist <= dist[v.node] {
			for _, e := range g[v.node].out {
				nd := v.dist + e.w
				if nd < dist[e.to] {
					dist[e.to] = nd
					pq.push(visit{e.to, nd})
				}
			}
		}
	}
	total := int32(0)
	for _, d := range dist {
		if d != 2147483647 {
			total += d
		}
	}
	return total
}

func benchGraph(iters int, class bool) {
	rng := lcg{41 + seedBit()}
	g := makeGraph(&rng, 2000, 6)
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		src := int32((i * 37) % 2000)
		if class {
			acc += shortestClass(g, src)
		} else {
			acc += shortestValue(g, src)
		}
	}
	sinkInt(acc)
	name := "graph_paths_value"
	if class {
		name = "graph_paths_class"
	}
	report(name, start, iters, acc)
}

func leaf(i int32) int32 { return (i * 31) & 1023 }

func benchFanout(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		var wg sync.WaitGroup
		out := make([]int32, 64)
		wg.Add(64)
		for k := 0; k < 64; k++ {
			k := k
			base := int32(i + k*4)
			go func() {
				parts := make([]int32, 4)
				var inner sync.WaitGroup
				inner.Add(4)
				for p := 0; p < 4; p++ {
					p := p
					go func() {
						v := base + int32(p)
						if v&7 == 0 {
							time.Sleep(0)
						}
						parts[p] = leaf(v)
						inner.Done()
					}()
				}
				inner.Wait()
				out[k] = parts[0] + parts[1] + parts[2] + parts[3]
				wg.Done()
			}()
		}
		wg.Wait()
		for _, v := range out {
			acc += v
		}
		race := make(chan int32, 2)
		go func() { race <- leaf(int32(i)) }()
		go func() { race <- leaf(int32(i + 1024)) }()
		acc += <-race
	}
	sinkInt(acc)
	report("async_fanout", start, iters, acc)
}

func chunkWork(seed int32) int32 {
	h := seed
	acc := int32(0)
	for i := 0; i < 20000; i++ {
		h = h*1103515245 + 12345
		acc += (h >> 16) & 255
	}
	return acc
}

func benchTasks(iters int) {
	bit := seedBit()
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		seeds := make([]int32, 16)
		for k := 0; k < 16; k++ {
			seeds[k] = int32(i*16+k) + bit
		}
		var wg sync.WaitGroup
		out := make([]int32, 16)
		wg.Add(16)
		for k := 0; k < 16; k++ {
			k := k
			go func() {
				out[k] = chunkWork(seeds[k])
				wg.Done()
			}()
		}
		wg.Wait()
		sum := int32(0)
		for _, v := range out {
			sum += v
		}
		acc += sum
	}
	sinkInt(acc)
	report("task_parallel", start, iters, acc)
}

func runSuite(s int) {
	benchJSON(s * 20)
	benchLog(s / 10)
	benchGraph(s/4, true)
	benchGraph(s/4, false)
	benchFanout(s * 2)
	benchTasks(s / 2)
}

func main() {
	measuring = false
	runSuite(scale)
	measuring = true
	runSuite(scale)
	os.Stdout.WriteString("sink " + strconv.FormatInt(int64(sink), 10) + "\n")
}
