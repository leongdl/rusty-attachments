#!/bin/bash
# Monitor memory usage of a process
PID=$1
OUTPUT=$2

echo "timestamp,rss_mb,vms_mb" > $OUTPUT

while kill -0 $PID 2>/dev/null; do
    RSS=$(cat /proc/$PID/status 2>/dev/null | grep VmRSS | awk '{print $2/1024}')
    VMS=$(cat /proc/$PID/status 2>/dev/null | grep VmSize | awk '{print $2/1024}')
    if [ -n "$RSS" ]; then
        echo "$(date +%s),$RSS,$VMS" >> $OUTPUT
    fi
    sleep 0.5
done
