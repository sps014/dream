// Go counterpart of tests/bench/microbenches.dream.
// Same scale, iteration counts, and 32-bit wrap. Strings are Go UTF-8; Dream's are UTF-16.
package main

import (
	"encoding/json"
	"math"
	"os"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"time"
	"weak"
)

const scale = 20000

var measuring = true
var benchSink int32

//go:noinline
func sinkInt(x int32) int32 {
	benchSink += x
	return x
}

//go:noinline
func sinkString(s string) int32 {
	n := int32(len(s))
	benchSink += n
	return n
}

func report(name string, start time.Time, iters int, checksum int32) {
	if !measuring {
		return
	}
	ns := time.Since(start).Nanoseconds()
	per := int64(0)
	if iters != 0 {
		per = ns / int64(iters)
	}
	os.Stdout.WriteString("bench " + name + " ns_total=" + strconv.FormatInt(ns, 10) +
		" iters=" + strconv.Itoa(iters) + " ns_per_op=" + strconv.FormatInt(per, 10) +
		" checksum=" + strconv.FormatInt(int64(checksum), 10) + "\n")
}

func envBit() int32 {
	v := os.Getenv("DREAM_BENCH_SEED")
	if v == "" {
		return 1
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		return 1
	}
	return int32(n) & 1
}

func benchNbody(iters int) {
	type body struct{ x, y, z, vx, vy, vz, mass float64 }
	bodies := []body{
		{0, 0, 0, 0.01, 0, 0, 1},
		{1, 0.5, 0.2, 0, 0.02, 0, 0.5},
		{-1.2, 0.3, -0.4, 0.01, 0, 0.03, 0.25},
		{0.4, -0.9, 0.8, -0.02, 0.01, 0, 0.125},
		{0.9, 0.9, -0.7, 0, -0.01, 0.02, 0.0625},
	}
	n := len(bodies)
	const dt = 0.01
	start := time.Now()
	for i := 0; i < iters; i++ {
		for a := 0; a < n; a++ {
			for b := a + 1; b < n; b++ {
				dx := bodies[b].x - bodies[a].x
				dy := bodies[b].y - bodies[a].y
				dz := bodies[b].z - bodies[a].z
				d2 := dx*dx + dy*dy + dz*dz + 1e-9
				inv := 1.0 / math.Sqrt(d2)
				f := bodies[b].mass * bodies[a].mass * inv * inv * inv * dt
				bodies[a].vx += dx * f
				bodies[a].vy += dy * f
				bodies[a].vz += dz * f
				bodies[b].vx -= dx * f
				bodies[b].vy -= dy * f
				bodies[b].vz -= dz * f
			}
		}
		for k := 0; k < n; k++ {
			bodies[k].x += bodies[k].vx * dt
			bodies[k].y += bodies[k].vy * dt
			bodies[k].z += bodies[k].vz * dt
		}
	}
	checksum := int32(bodies[0].x * 1000)
	report("nbody", start, iters, checksum)
	sinkInt(checksum)
}

func benchMandelbrot(iters int) {
	jitter := float64(envBit())
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		for row := 0; row < 48; row++ {
			ci := float64(row)/24.0 - 1.0
			for col := 0; col < 64; col++ {
				cr := (float64(col)+jitter)/32.0 - 1.5
				zr, zi := 0.0, 0.0
				k := 0
				escaped := false
				for k < 30 && !escaped {
					zr2 := zr*zr - zi*zi + cr
					zi = 2*zr*zi + ci
					zr = zr2
					if zr*zr+zi*zi > 4 {
						escaped = true
					}
					k++
				}
				acc += int32(k)
			}
		}
	}
	report("mandelbrot", start, iters, acc)
	sinkInt(acc)
}

func benchMatmul(iters int) {
	const n = 64
	a := make([]float64, n*n)
	b := make([]float64, n*n)
	c := make([]float64, n*n)
	for t := 0; t < n*n; t++ {
		a[t] = float64(t % 13)
		b[t] = float64((t * 7) % 11)
	}
	start := time.Now()
	for r := 0; r < iters; r++ {
		for i := 0; i < n; i++ {
			for k := 0; k < n; k++ {
				aik := a[i*n+k]
				for j := 0; j < n; j++ {
					c[i*n+j] += aik * b[k*n+j]
				}
			}
		}
	}
	checksum := int32(c[0])
	report("matmul_64", start, iters, checksum)
	sinkInt(checksum)
}

func qsortRange(a []int32, lo, hi int) {
	if lo >= hi {
		return
	}
	p := a[(lo+hi)/2]
	i, j := lo, hi
	for i <= j {
		for a[i] < p {
			i++
		}
		for a[j] > p {
			j--
		}
		if i <= j {
			a[i], a[j] = a[j], a[i]
			i++
			j--
		}
	}
	qsortRange(a, lo, j)
	qsortRange(a, i, hi)
}

func benchQuicksort(iters int) {
	const n = 512
	a := make([]int32, n)
	seed := int32(123456789)
	start := time.Now()
	for r := 0; r < iters; r++ {
		for t := 0; t < n; t++ {
			seed = seed*1103515245 + 12345
			a[t] = (seed >> 16) & 1023
		}
		qsortRange(a, 0, n-1)
	}
	report("quicksort", start, iters, a[0])
	sinkInt(a[0])
}

func benchSieve(iters int) {
	const n = 4096
	flags := make([]int32, n)
	start := time.Now()
	acc := int32(0)
	for r := 0; r < iters; r++ {
		for t := 0; t < n; t++ {
			flags[t] = 1
		}
		for i := 2; i*i < n; i++ {
			if flags[i] == 1 {
				for m := i * i; m < n; m += i {
					flags[m] = 0
				}
			}
		}
		for p := 2; p < n; p++ {
			acc += flags[p]
		}
	}
	report("sieve", start, iters, acc)
	sinkInt(acc)
}

func fib(n int32) int32 {
	if n < 2 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

func benchFib(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		acc += fib(20 + int32(i&1))
	}
	report("fib_rec", start, iters, acc)
	sinkInt(acc)
}

type benchOp interface{ apply(x int32) int32 }

type addOp struct{}
type mulOp struct{}
type subOp struct{}
type xorOp struct{}

func (addOp) apply(x int32) int32 { return x + 3 }
func (mulOp) apply(x int32) int32 { return x * 3 }
func (subOp) apply(x int32) int32 { return x - 3 }
func (xorOp) apply(x int32) int32 { return x ^ 3 }

//go:noinline
func ifaceRounds(ops []benchOp, rounds int) int32 {
	acc := int32(0)
	for r := 0; r < rounds; r++ {
		rv := int32(r)
		for _, op := range ops {
			acc += op.apply(rv)
		}
	}
	return acc
}

func benchIface(iters int) {
	const n = 1024
	ops := make([]benchOp, 0, n)
	seed := int32(12345)
	for k := 0; k < n; k++ {
		seed = seed*1103515245 + 12345
		switch (seed >> 16) & 3 {
		case 0:
			ops = append(ops, addOp{})
		case 1:
			ops = append(ops, mulOp{})
		case 2:
			ops = append(ops, subOp{})
		default:
			ops = append(ops, xorOp{})
		}
	}
	rounds := iters * 3 / n
	if rounds < 1 {
		rounds = 1
	}
	start := time.Now()
	acc := ifaceRounds(ops, rounds)
	report("iface_dispatch", start, rounds*n, acc)
	sinkInt(acc)
}

//go:noinline
func closureOffset(offset int32) func(int32) int32 {
	return func(value int32) int32 { return value + offset }
}

//go:noinline
func closureCalls(callback func(int32) int32, iters int) int32 {
	total := int32(0)
	for i := 0; i < iters; i++ {
		total += callback(int32(i))
	}
	return total
}

func benchClosure(iters int) {
	callback := closureOffset(envBit())
	start := time.Now()
	total := closureCalls(callback, iters)
	sinkInt(total)
	report("closure_dispatch", start, iters, total)
}

type treeNode struct {
	left, right *treeNode
}

func makeTree(depth int) *treeNode {
	if depth <= 0 {
		return nil
	}
	return &treeNode{makeTree(depth - 1), makeTree(depth - 1)}
}

func benchTreesAlloc(iters int) {
	keep := make([]*treeNode, 0, iters)
	start := time.Now()
	for i := 0; i < iters; i++ {
		keep = append(keep, makeTree(12))
	}
	report("binary_trees_alloc", start, iters, int32(len(keep)))
	sinkInt(int32(len(keep)))
}

func benchTrees(iters int, reclaim bool) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		if makeTree(12) != nil {
			acc++
		} else {
			acc--
		}
		if reclaim {
			runtime.GC()
		}
	}
	label := "binary_trees"
	if reclaim {
		label = "binary_trees_reclaim"
	}
	report(label, start, iters, acc)
	sinkInt(acc)
}

type linkNode struct {
	value int32
	next  *linkNode
}

func makeChain(n int) *linkNode {
	var head *linkNode
	for i := 0; i < n; i++ {
		head = &linkNode{int32(i), head}
	}
	return head
}

func benchLinked(iters int) {
	head := makeChain(1024)
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		for curr := head; curr != nil; curr = curr.next {
			acc += curr.value
		}
	}
	report("linked_walk", start, iters, acc)
	sinkInt(acc)
}

type weakNode struct {
	left, right *weakNode
	parent      weak.Pointer[weakNode]
}

func makeWeakTree(depth int) *weakNode {
	node := &weakNode{}
	if depth > 1 {
		l := makeWeakTree(depth - 1)
		l.parent = weak.Make(node)
		r := makeWeakTree(depth - 1)
		r.parent = weak.Make(node)
		node.left = l
		node.right = r
	}
	return node
}

func benchWeak(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		root := makeWeakTree(10)
		if root.left != nil {
			acc++
		} else {
			acc--
		}
	}
	report("weak_tree", start, iters, acc)
	sinkInt(acc)
}

func benchWordcount(iters int) {
	text := "the quick brown fox jumps over the lazy dog the end the fox and the dog run with the quick fox"
	words := strings.Split(text, " ")
	m := make(map[string]int32, 256)
	start := time.Now()
	for i := 0; i < iters; i++ {
		for _, w := range words {
			m[w] = m[w] + 1
		}
	}
	report("wordcount", start, iters*len(words), m["the"])
	sinkInt(m["the"])
}

func benchParse(iters int) {
	seed := envBit()
	srcs := make([]string, 8)
	for k := 0; k < 8; k++ {
		srcs[k] = strconv.FormatInt(int64(1000000000+int32(k)*111111+seed), 10)
	}
	start := time.Now()
	acc := int64(0)
	for i := 0; i < iters; i++ {
		src := srcs[i&7]
		v := int32(0)
		for j := 0; j < len(src); j++ {
			v = v*10 + int32(src[j]) - 48
		}
		acc += int64(v)
	}
	checksum := int32(acc)
	report("parse_ints", start, iters, checksum)
	sinkInt(checksum)
}

func benchOptions(iters int) {
	seed := envBit()
	opts := make([]struct {
		some bool
		v    int32
	}, 8)
	for k := 0; k < 8; k++ {
		if ((int32(k) + seed) & 1) == 0 {
			opts[k].some = true
			opts[k].v = int32(k + 7)
		}
	}
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		a := opts[i&7]
		if a.some {
			acc += a.v
		} else {
			acc--
		}
		b := opts[(i+3)&7]
		if b.some {
			acc += b.v
		} else {
			acc++
		}
	}
	report("sum_options", start, iters, acc)
	sinkInt(acc)
}

func benchArc(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		a := "n" + strconv.Itoa(i)
		b := a
		c := b
		acc += int32(len(c))
	}
	report("arc_locals", start, iters, acc)
	sinkInt(acc)
}

func benchConcat(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		s := "hello" + strconv.Itoa(i) + "world"
		acc += int32(len(s))
	}
	report("string_concat", start, iters, acc)
	sinkInt(acc)
}

func benchEq(iters int) {
	base := "abcdefghijklmnopqrstuvwxyz0123456789"
	variants := []string{
		"abcdefghijklmnopqrstuvwxyz0123456789",
		"abcdefghijklmnopqrstuvwxyz0123456780",
		"abcdefghijKlmnopqrstuvwxyz0123456789",
		"abcdefghijklmnopqrstuvwxyz012345678",
	}
	start := time.Now()
	hits := int32(0)
	for i := 0; i < iters; i++ {
		if base == variants[i&3] {
			hits++
		} else {
			hits--
		}
		sinkInt(hits)
	}
	report("string_eq", start, iters, hits)
	sinkInt(hits)
}

func benchCharScan(iters int) {
	lit := "The quick brown fox jumps over the lazy dog. 0123456789"
	s := lit[0:len(lit)]
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		n := len(s)
		for bo := 0; bo < n; bo++ {
			acc += int32(s[bo])
		}
	}
	report("char_scan", start, iters, acc)
	sinkInt(acc)
}

func benchByteScan(iters int) {
	lit := "The quick brown fox jumps over the lazy dog. 0123456789"
	s := lit[0:len(lit)]
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		n := len(s)
		for j := 0; j < n; j++ {
			acc += int32(s[j])
		}
	}
	report("byte_scan", start, iters, acc)
	sinkInt(acc)
}

func benchSubstring(iters int) {
	s := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789" + strconv.Itoa(int(envBit()))
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		st := i & 7
		sub := s[st : st+25+(i&3)]
		acc += int32(len(sub)) + int32(sub[0])
	}
	report("substring", start, iters, acc)
	sinkInt(acc)
}

func benchSpan(iters int) {
	s := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789" + strconv.Itoa(int(envBit()))
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		st := i & 7
		sub := s[st : st+25+(i&3)]
		acc += int32(len(sub)) + int32(sub[0])
	}
	report("substring_span", start, iters, acc)
	sinkInt(acc)
}

func benchSplit(iters int) {
	line := "alpha,beta,gamma,delta,epsilon,zeta,eta,theta"
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		from := 0
		for j := 0; j <= len(line); j++ {
			if j == len(line) || line[j] == ',' {
				acc += int32(j - from)
				from = j + 1
			}
		}
	}
	report("split_span", start, iters, acc)
	sinkInt(acc)
}

func benchMapSpan(iters int) {
	text := "k0k1k2k3k4k5k6k7"
	m := map[string]int32{}
	for j := 0; j < 8; j++ {
		m["k"+strconv.Itoa(j)] = int32(j + 1)
	}
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		k := (i & 7) * 2
		acc += m[text[k:k+2]]
	}
	report("map_get_span", start, iters, acc)
	sinkInt(acc)
}

func benchListPush(iters int) {
	start := time.Now()
	list := make([]int32, 0, iters)
	for i := 0; i < iters; i++ {
		list = append(list, int32(i))
	}
	report("list_push", start, iters, int32(len(list)))
	sinkInt(int32(len(list)))
}

func benchInsertMid(iters int) {
	const n = 256
	rounds := iters / n
	if rounds < 1 {
		rounds = 1
	}
	start := time.Now()
	acc := int32(0)
	for r := 0; r < rounds; r++ {
		list := make([]int32, 0, n)
		for i := 0; i < n; i++ {
			mid := len(list) / 2
			list = append(list, 0)
			copy(list[mid+1:], list[mid:])
			list[mid] = int32(i)
		}
		acc += list[len(list)/2]
	}
	report("list_insert_mid", start, rounds*n, acc)
}

func benchMap(iters int) {
	m := make(map[int32]int32, iters)
	start := time.Now()
	for i := 0; i < iters; i++ {
		m[int32(i)] = int32(i) * 3
	}
	acc := int32(0)
	for i := 0; i < iters; i++ {
		acc += m[int32(i)]
	}
	report("map_get_set", start, iters, acc)
	sinkInt(acc)
}

func benchMapClear(iters int) {
	m := make(map[int32]int32, 1024)
	const rounds = 64
	per := iters / rounds
	if per < 1 {
		per = 1
	}
	start := time.Now()
	for r := 0; r < rounds; r++ {
		for i := 0; i < per; i++ {
			m[int32(i)] = int32(i)
		}
		clear(m)
	}
	report("map_clear_reuse", start, rounds*per, int32(len(m)))
	sinkInt(int32(len(m)))
}

func benchSet(iters int) {
	set := map[int32]struct{}{}
	start := time.Now()
	for i := 0; i < iters; i++ {
		set[int32(i)] = struct{}{}
	}
	acc := int32(0)
	for i := 0; i < iters; i++ {
		if _, ok := set[int32(i)]; ok {
			acc++
		}
		if _, ok := set[int32(i)]; ok {
			delete(set, int32(i))
			acc++
		}
	}
	report("set_probe", start, iters, acc+int32(len(set)))
	sinkInt(acc + int32(len(set)))
}

func benchSetClear(iters int) {
	set := make(map[int32]struct{}, 1024)
	const rounds = 64
	per := iters / rounds
	if per < 1 {
		per = 1
	}
	start := time.Now()
	for r := 0; r < rounds; r++ {
		for i := 0; i < per; i++ {
			set[int32(i)] = struct{}{}
		}
		clear(set)
	}
	report("set_clear_reuse", start, rounds*per, int32(len(set)))
	sinkInt(int32(len(set)))
}

func benchListClear(iters int) {
	buf := make([]int32, 0, 64)
	const rounds = 64
	per := iters / rounds
	if per < 1 {
		per = 1
	}
	start := time.Now()
	for r := 0; r < rounds; r++ {
		for i := 0; i < per; i++ {
			buf = append(buf, int32(i))
		}
		buf = buf[:0]
	}
	report("list_clear_reuse", start, rounds*per, int32(len(buf)))
	sinkInt(int32(cap(buf)))
}

//go:noinline
func touchBuf(buf []int32) int32 { return buf[0] }

func benchAlloc(iters int) {
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		buf := make([]int32, 64)
		buf[0] = int32(i)
		acc += touchBuf(buf)
	}
	report("alloc_churn", start, iters, acc)
	sinkInt(acc)
}

func benchRegex(iters int) {
	re := regexp.MustCompile(`[a-z]+\d+`)
	hay := "abc123def456ghi789xyz"
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		ms := re.FindAllString(hay, -1)
		for _, m := range ms {
			acc += int32(len(m))
		}
	}
	report("regex_find", start, iters, acc)
	sinkInt(acc)
}

type benchAddress struct {
	City string `json:"city"`
	Zip  string `json:"zip"`
}
type benchUser struct {
	Name    string       `json:"name"`
	Age     int32        `json:"age"`
	Active  bool         `json:"active"`
	Address benchAddress `json:"address"`
	Tags    []string     `json:"tags"`
	Scores  []int32      `json:"scores"`
}

func user() benchUser {
	return benchUser{"Ada", 36, true, benchAddress{"London", "NW1"}, []string{"dev", "math"}, []int32{10, 20, 30}}
}

func benchJSONSer(iters int) {
	u := user()
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		text, err := json.Marshal(u)
		if err != nil {
			panic(err)
		}
		acc += int32(len(text))
	}
	report("json_serialize", start, iters, acc)
	sinkInt(acc)
}

func benchJSONDe(iters int) {
	text, err := json.Marshal(user())
	if err != nil {
		panic(err)
	}
	fallback := user()
	start := time.Now()
	acc := int32(0)
	for i := 0; i < iters; i++ {
		var back benchUser
		err := json.Unmarshal(text, &back)
		if err != nil {
			back = fallback
		}
		acc += back.Age + back.Scores[2]
	}
	report("json_deserialize", start, iters, acc)
	sinkInt(acc)
}

func benchArr(iters int) {
	const n = 256
	a := make([]float32, n)
	b := make([]float32, n)
	ai := make([]int32, n)
	bi := make([]int32, n)
	for k := 0; k < n; k++ {
		a[k] = float32(k)
		b[k] = float32(k + 1)
		ai[k] = int32(k)
		bi[k] = int32(k + 1)
	}
	start := time.Now()
	for r := 0; r < iters; r++ {
		for i := 0; i < n; i++ {
			a[i] = a[i] + b[i]
		}
		for j := 0; j < n; j++ {
			ai[j] = ai[j] + bi[j]
		}
	}
	acc := int32(0)
	for i := 0; i < n; i++ {
		acc += int32(a[i]) + ai[i]
	}
	report("arr_add", start, iters, acc)
	sinkInt(acc)
}

func benchVec(iters int) {
	const n = 256
	a := make([]float32, n)
	b := make([]float32, n)
	for k := 0; k < n; k++ {
		a[k] = float32(k)
		b[k] = float32(k + 1)
	}
	start := time.Now()
	for r := 0; r < iters; r++ {
		for i := 0; i < n; i++ {
			a[i] = a[i] + b[i]
		}
	}
	acc := int32(0)
	for i := 0; i < n; i++ {
		acc += int32(a[i])
	}
	report("vec_add", start, iters, acc)
	sinkInt(acc)
}

func benchBuilder(iters int) {
	chunk := "abcdefghijklmnopqrstuvwxyz"
	var sb strings.Builder
	sb.Grow(iters * len(chunk))
	start := time.Now()
	for i := 0; i < iters; i++ {
		sb.WriteString(chunk)
	}
	built := sb.String()
	report("string_builder", start, iters, int32(len(built)))
	sinkString(built)
}

func runSuite(s int) {
	benchNbody(s * 4)
	benchMandelbrot(s / 100)
	benchMatmul(s / 50)
	benchQuicksort(s / 20)
	benchSieve(s / 12)
	benchFib(s / 20)
	benchIface(s * 12)
	benchClosure(s * 12)
	benchTrees(s/200, false)
	benchTreesAlloc(s / 200)
	benchLinked(s / 5)
	benchWeak(s / 200)
	benchWordcount(s)
	benchParse(s * 50)
	benchOptions(s * 400)
	benchArc(s * 25)
	benchConcat(s * 20)
	benchEq(s * 100)
	benchCharScan(s * 10)
	benchByteScan(s * 20)
	benchSubstring(s * 160)
	benchSpan(s * 160)
	benchSplit(s * 20)
	benchListPush(s * 400)
	benchInsertMid(s * 40)
	benchMap(s * 35)
	benchMapSpan(s * 35)
	benchMapClear(s * 70)
	benchSet(s * 35)
	benchSetClear(s * 70)
	benchListClear(s * 500)
	benchAlloc(s * 30)
	benchRegex(s / 2)
	benchBuilder(s * 20)
	benchJSONSer(s * 2)
	benchJSONDe(s / 2)
	benchArr(s * 5)
	benchVec(s * 10)
	benchTrees(s/200, true)
}

func benchAsync(iters int) {
	start := time.Now()
	total := int32(0)
	for i := 0; i < iters; i++ {
		ch := make(chan int32, 1)
		go func(v int32) { ch <- v + 1 }(int32(i))
		total += <-ch
	}
	sinkInt(total)
	report("async_poll", start, iters, total)
}

func benchSched(iters int) {
	start := time.Now()
	total := int32(0)
	for i := 0; i < iters; i++ {
		ch := make(chan int32, 2)
		go func(v int32) { ch <- v + 1 }(int32(i))
		go func(v int32) { ch <- v + 1 }(int32(i + 1))
		total += <-ch + <-ch
	}
	sinkInt(total)
	report("scheduler_queue", start, iters, total)
}

func main() {
	measuring = false
	runSuite(scale)
	benchAsync(scale)
	benchSched(scale / 4)
	measuring = true
	runSuite(scale)
	benchAsync(scale)
	benchSched(scale / 4)
	os.Stdout.WriteString("sink " + strconv.FormatInt(int64(benchSink), 10) + "\n")
}
