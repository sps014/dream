	.hidden	__dream_g0
	.globaltype	__dream_g0, i32
__dream_g0:

	.hidden	__dream_tid
	.globaltype	__dream_tid, i32
__dream_tid:

	.hidden	__dream_priv_slab
	.globaltype	__dream_priv_slab, i32
__dream_priv_slab:

	.hidden	__dream_priv_off
	.globaltype	__dream_priv_off, i32
__dream_priv_off:

	.hidden	__dream_priv_cap
	.globaltype	__dream_priv_cap, i32
__dream_priv_cap:

	.hidden	__dream_priv_fl0
	.globaltype	__dream_priv_fl0, i32
__dream_priv_fl0:
	.hidden	__dream_priv_fl1
	.globaltype	__dream_priv_fl1, i32
__dream_priv_fl1:
	.hidden	__dream_priv_fl2
	.globaltype	__dream_priv_fl2, i32
__dream_priv_fl2:
	.hidden	__dream_priv_fl3
	.globaltype	__dream_priv_fl3, i32
__dream_priv_fl3:
	.hidden	__dream_priv_fl4
	.globaltype	__dream_priv_fl4, i32
__dream_priv_fl4:
	.hidden	__dream_priv_fl5
	.globaltype	__dream_priv_fl5, i32
__dream_priv_fl5:
	.hidden	__dream_priv_fl6
	.globaltype	__dream_priv_fl6, i32
__dream_priv_fl6:
	.hidden	__dream_priv_fl7
	.globaltype	__dream_priv_fl7, i32
__dream_priv_fl7:
	.hidden	__dream_priv_fl8
	.globaltype	__dream_priv_fl8, i32
__dream_priv_fl8:
	.hidden	__dream_priv_fl9
	.globaltype	__dream_priv_fl9, i32
__dream_priv_fl9:
	.hidden	__dream_priv_fl10
	.globaltype	__dream_priv_fl10, i32
__dream_priv_fl10:
	.hidden	__dream_priv_fl11
	.globaltype	__dream_priv_fl11, i32
__dream_priv_fl11:
	.hidden	__dream_priv_fl12
	.globaltype	__dream_priv_fl12, i32
__dream_priv_fl12:

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

	.globl	dream_priv_slab_get
	.type	dream_priv_slab_get,@function
dream_priv_slab_get:
	.functype	dream_priv_slab_get () -> (i32)
	global.get	__dream_priv_slab
	end_function

	.globl	dream_priv_slab_set
	.type	dream_priv_slab_set,@function
dream_priv_slab_set:
	.functype	dream_priv_slab_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_slab
	end_function

	.globl	dream_priv_off_get
	.type	dream_priv_off_get,@function
dream_priv_off_get:
	.functype	dream_priv_off_get () -> (i32)
	global.get	__dream_priv_off
	end_function

	.globl	dream_priv_off_set
	.type	dream_priv_off_set,@function
dream_priv_off_set:
	.functype	dream_priv_off_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_off
	end_function

	.globl	dream_priv_cap_get
	.type	dream_priv_cap_get,@function
dream_priv_cap_get:
	.functype	dream_priv_cap_get () -> (i32)
	global.get	__dream_priv_cap
	end_function

	.globl	dream_priv_cap_set
	.type	dream_priv_cap_set,@function
dream_priv_cap_set:
	.functype	dream_priv_cap_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_cap
	end_function

	.globl	dream_priv_fl0_get
	.type	dream_priv_fl0_get,@function
dream_priv_fl0_get:
	.functype	dream_priv_fl0_get () -> (i32)
	global.get	__dream_priv_fl0
	end_function
	.globl	dream_priv_fl0_set
	.type	dream_priv_fl0_set,@function
dream_priv_fl0_set:
	.functype	dream_priv_fl0_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl0
	end_function

	.globl	dream_priv_fl1_get
	.type	dream_priv_fl1_get,@function
dream_priv_fl1_get:
	.functype	dream_priv_fl1_get () -> (i32)
	global.get	__dream_priv_fl1
	end_function
	.globl	dream_priv_fl1_set
	.type	dream_priv_fl1_set,@function
dream_priv_fl1_set:
	.functype	dream_priv_fl1_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl1
	end_function

	.globl	dream_priv_fl2_get
	.type	dream_priv_fl2_get,@function
dream_priv_fl2_get:
	.functype	dream_priv_fl2_get () -> (i32)
	global.get	__dream_priv_fl2
	end_function
	.globl	dream_priv_fl2_set
	.type	dream_priv_fl2_set,@function
dream_priv_fl2_set:
	.functype	dream_priv_fl2_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl2
	end_function

	.globl	dream_priv_fl3_get
	.type	dream_priv_fl3_get,@function
dream_priv_fl3_get:
	.functype	dream_priv_fl3_get () -> (i32)
	global.get	__dream_priv_fl3
	end_function
	.globl	dream_priv_fl3_set
	.type	dream_priv_fl3_set,@function
dream_priv_fl3_set:
	.functype	dream_priv_fl3_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl3
	end_function

	.globl	dream_priv_fl4_get
	.type	dream_priv_fl4_get,@function
dream_priv_fl4_get:
	.functype	dream_priv_fl4_get () -> (i32)
	global.get	__dream_priv_fl4
	end_function
	.globl	dream_priv_fl4_set
	.type	dream_priv_fl4_set,@function
dream_priv_fl4_set:
	.functype	dream_priv_fl4_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl4
	end_function

	.globl	dream_priv_fl5_get
	.type	dream_priv_fl5_get,@function
dream_priv_fl5_get:
	.functype	dream_priv_fl5_get () -> (i32)
	global.get	__dream_priv_fl5
	end_function
	.globl	dream_priv_fl5_set
	.type	dream_priv_fl5_set,@function
dream_priv_fl5_set:
	.functype	dream_priv_fl5_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl5
	end_function

	.globl	dream_priv_fl6_get
	.type	dream_priv_fl6_get,@function
dream_priv_fl6_get:
	.functype	dream_priv_fl6_get () -> (i32)
	global.get	__dream_priv_fl6
	end_function
	.globl	dream_priv_fl6_set
	.type	dream_priv_fl6_set,@function
dream_priv_fl6_set:
	.functype	dream_priv_fl6_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl6
	end_function

	.globl	dream_priv_fl7_get
	.type	dream_priv_fl7_get,@function
dream_priv_fl7_get:
	.functype	dream_priv_fl7_get () -> (i32)
	global.get	__dream_priv_fl7
	end_function
	.globl	dream_priv_fl7_set
	.type	dream_priv_fl7_set,@function
dream_priv_fl7_set:
	.functype	dream_priv_fl7_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl7
	end_function

	.globl	dream_priv_fl8_get
	.type	dream_priv_fl8_get,@function
dream_priv_fl8_get:
	.functype	dream_priv_fl8_get () -> (i32)
	global.get	__dream_priv_fl8
	end_function
	.globl	dream_priv_fl8_set
	.type	dream_priv_fl8_set,@function
dream_priv_fl8_set:
	.functype	dream_priv_fl8_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl8
	end_function

	.globl	dream_priv_fl9_get
	.type	dream_priv_fl9_get,@function
dream_priv_fl9_get:
	.functype	dream_priv_fl9_get () -> (i32)
	global.get	__dream_priv_fl9
	end_function
	.globl	dream_priv_fl9_set
	.type	dream_priv_fl9_set,@function
dream_priv_fl9_set:
	.functype	dream_priv_fl9_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl9
	end_function

	.globl	dream_priv_fl10_get
	.type	dream_priv_fl10_get,@function
dream_priv_fl10_get:
	.functype	dream_priv_fl10_get () -> (i32)
	global.get	__dream_priv_fl10
	end_function
	.globl	dream_priv_fl10_set
	.type	dream_priv_fl10_set,@function
dream_priv_fl10_set:
	.functype	dream_priv_fl10_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl10
	end_function

	.globl	dream_priv_fl11_get
	.type	dream_priv_fl11_get,@function
dream_priv_fl11_get:
	.functype	dream_priv_fl11_get () -> (i32)
	global.get	__dream_priv_fl11
	end_function
	.globl	dream_priv_fl11_set
	.type	dream_priv_fl11_set,@function
dream_priv_fl11_set:
	.functype	dream_priv_fl11_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl11
	end_function

	.globl	dream_priv_fl12_get
	.type	dream_priv_fl12_get,@function
dream_priv_fl12_get:
	.functype	dream_priv_fl12_get () -> (i32)
	global.get	__dream_priv_fl12
	end_function
	.globl	dream_priv_fl12_set
	.type	dream_priv_fl12_set,@function
dream_priv_fl12_set:
	.functype	dream_priv_fl12_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fl12
	end_function

	.hidden	__dream_region_depth
	.globaltype	__dream_region_depth, i32
__dream_region_depth:
	.hidden	__dream_region_slab
	.globaltype	__dream_region_slab, i32
__dream_region_slab:
	.hidden	__dream_region_cap
	.globaltype	__dream_region_cap, i32
__dream_region_cap:
	.hidden	__dream_region_off
	.globaltype	__dream_region_off, i32
__dream_region_off:
	.hidden	__dream_region_nalloc
	.globaltype	__dream_region_nalloc, i32
__dream_region_nalloc:
	.hidden	__dream_region_marks
	.globaltype	__dream_region_marks, i32
__dream_region_marks:

	.globl	dream_region_depth_get
	.type	dream_region_depth_get,@function
dream_region_depth_get:
	.functype	dream_region_depth_get () -> (i32)
	global.get	__dream_region_depth
	end_function
	.globl	dream_region_depth_set
	.type	dream_region_depth_set,@function
dream_region_depth_set:
	.functype	dream_region_depth_set (i32) -> ()
	local.get	0
	global.set	__dream_region_depth
	end_function

	.globl	dream_region_slab_get
	.type	dream_region_slab_get,@function
dream_region_slab_get:
	.functype	dream_region_slab_get () -> (i32)
	global.get	__dream_region_slab
	end_function
	.globl	dream_region_slab_set
	.type	dream_region_slab_set,@function
dream_region_slab_set:
	.functype	dream_region_slab_set (i32) -> ()
	local.get	0
	global.set	__dream_region_slab
	end_function

	.globl	dream_region_cap_get
	.type	dream_region_cap_get,@function
dream_region_cap_get:
	.functype	dream_region_cap_get () -> (i32)
	global.get	__dream_region_cap
	end_function
	.globl	dream_region_cap_set
	.type	dream_region_cap_set,@function
dream_region_cap_set:
	.functype	dream_region_cap_set (i32) -> ()
	local.get	0
	global.set	__dream_region_cap
	end_function

	.globl	dream_region_off_get
	.type	dream_region_off_get,@function
dream_region_off_get:
	.functype	dream_region_off_get () -> (i32)
	global.get	__dream_region_off
	end_function
	.globl	dream_region_off_set
	.type	dream_region_off_set,@function
dream_region_off_set:
	.functype	dream_region_off_set (i32) -> ()
	local.get	0
	global.set	__dream_region_off
	end_function

	.globl	dream_region_nalloc_get
	.type	dream_region_nalloc_get,@function
dream_region_nalloc_get:
	.functype	dream_region_nalloc_get () -> (i32)
	global.get	__dream_region_nalloc
	end_function
	.globl	dream_region_nalloc_set
	.type	dream_region_nalloc_set,@function
dream_region_nalloc_set:
	.functype	dream_region_nalloc_set (i32) -> ()
	local.get	0
	global.set	__dream_region_nalloc
	end_function

	.globl	dream_region_marks_get
	.type	dream_region_marks_get,@function
dream_region_marks_get:
	.functype	dream_region_marks_get () -> (i32)
	global.get	__dream_region_marks
	end_function
	.globl	dream_region_marks_set
	.type	dream_region_marks_set,@function
dream_region_marks_set:
	.functype	dream_region_marks_set (i32) -> ()
	local.get	0
	global.set	__dream_region_marks
	end_function
