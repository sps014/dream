package main

import "fmt"

func main() {
	total := 0
	for i := 0; i < 50000000; i++ {
		total += i
	}
	fmt.Println(total)
}
