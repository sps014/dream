	.hidden	__dream_g0
	.globaltype	__dream_g0, i32
__dream_g0:

	.hidden	__dream_tid
	.globaltype	__dream_tid, i32
__dream_tid:

	.globl	dream_g0_get
	.type	dream_g0_get,@function
dream_g0_get:
	.functype	dream_g0_get () -> (i32)
	global.get	__dream_g0
	end_function

	.globl	dream_g0_set
	.type	dream_g0_set,@function
dream_g0_set:
	.functype	dream_g0_set (i32) -> ()
	local.get	0
	global.set	__dream_g0
	end_function

	.globl	dream_tid_get
	.type	dream_tid_get,@function
dream_tid_get:
	.functype	dream_tid_get () -> (i32)
	global.get	__dream_tid
	end_function

	.globl	dream_tid_set
	.type	dream_tid_set,@function
dream_tid_set:
	.functype	dream_tid_set (i32) -> ()
	local.get	0
	global.set	__dream_tid
	end_function
