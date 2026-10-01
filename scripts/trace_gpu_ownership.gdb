set pagination off
set confirm off
break GpuTexture_create
run
finish
set $result = (char *)$rax
printf "Returned Result payload=%p rc=%d tag=%d child=%p child_rc=%d\n", $result, *(int *)($result - 4), *(int *)$result, *(void **)($result + 8), *(int *)(*(char **)($result + 8) - 4)
watch -l *(int *)($result - 4)
commands
silent
printf "Result rc=%d magic=%x\n", *(int *)($result - 4), *(unsigned int *)($result - 12)
backtrace 8
continue
end
continue
