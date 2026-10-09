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

	.hidden	__dream_priv_table
	.globaltype	__dream_priv_table, i32
__dream_priv_table:

	.hidden	__dream_priv_fast
	.globaltype	__dream_priv_fast, i32
__dream_priv_fast:

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

	.globl	dream_priv_table_get
	.type	dream_priv_table_get,@function
dream_priv_table_get:
	.functype	dream_priv_table_get () -> (i32)
	global.get	__dream_priv_table
	end_function

	.globl	dream_priv_table_set
	.type	dream_priv_table_set,@function
dream_priv_table_set:
	.functype	dream_priv_table_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_table
	end_function

	.globl	dream_priv_fast_get
	.type	dream_priv_fast_get,@function
dream_priv_fast_get:
	.functype	dream_priv_fast_get () -> (i32)
	global.get	__dream_priv_fast
	end_function

	.globl	dream_priv_fast_set
	.type	dream_priv_fast_set,@function
dream_priv_fast_set:
	.functype	dream_priv_fast_set (i32) -> ()
	local.get	0
	global.set	__dream_priv_fast
	end_function

	.hidden	__dream_region_state
	.globaltype	__dream_region_state, i32
__dream_region_state:
	.globl	dream_region_state_get
	.type	dream_region_state_get,@function
dream_region_state_get:
	.functype	dream_region_state_get () -> (i32)
	global.get	__dream_region_state
	end_function
	.globl	dream_region_state_set
	.type	dream_region_state_set,@function
dream_region_state_set:
	.functype	dream_region_state_set (i32) -> ()
	local.get	0
	global.set	__dream_region_state
	end_function

	.hidden	__dream_visit_context
	.globaltype	__dream_visit_context, i32
__dream_visit_context:
	.globl	dream_visit_context_get
	.type	dream_visit_context_get,@function
dream_visit_context_get:
	.functype	dream_visit_context_get () -> (i32)
	global.get	__dream_visit_context
	end_function
	.globl	dream_visit_context_set
	.type	dream_visit_context_set,@function
dream_visit_context_set:
	.functype	dream_visit_context_set (i32) -> ()
	local.get	0
	global.set	__dream_visit_context
	end_function

    .hidden __dream_weak_depth
    .globaltype __dream_weak_depth, i32
__dream_weak_depth:
    .globl dream_weak_depth_get
    .type dream_weak_depth_get,@function
dream_weak_depth_get:
    .functype dream_weak_depth_get () -> (i32)
    global.get __dream_weak_depth
    end_function
    .globl dream_weak_depth_set
    .type dream_weak_depth_set,@function
dream_weak_depth_set:
    .functype dream_weak_depth_set (i32) -> ()
    local.get 0
    global.set __dream_weak_depth
    end_function
