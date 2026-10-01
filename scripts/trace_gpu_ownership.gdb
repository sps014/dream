set pagination off
set confirm off
break GpuTexture_constructor
break Result_GpuTexture_GpuError_is_ok
commands 2
silent
set $result = (char *)$rdi
printf "Result rc=%d tag=%d child=%p child_rc=%d\n", *(int *)($result - 4), *(int *)$result, *(void **)($result + 8), *(int *)(*(char **)($result + 8) - 4)
continue
end
run
set $texture = (char *)$rdi
printf "Texture payload=%p initial_rc=%d\n", $texture, *(int *)($texture - 4)
watch -l *(int *)($texture - 4)
commands
silent
printf "Texture rc=%d magic=%x\n", *(int *)($texture - 4), *(unsigned int *)($texture - 12)
backtrace 8
continue
end
continue
