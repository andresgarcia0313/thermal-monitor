#!/bin/bash
# Thermal Manager v2.1 - Multi-distro compatible
# Automatic thermal management for keyboard comfort

detect_driver() {
    if [ -d /sys/devices/system/cpu/intel_pstate ]; then
        echo "intel_pstate"
    elif [ -d /sys/devices/system/cpu/amd_pstate ]; then
        echo "amd_pstate"
    else
        echo "cpufreq"
    fi
}

DRIVER=$(detect_driver)

get_cpu_temp() {
    local max_temp=0
    for zone in /sys/class/thermal/thermal_zone*/temp; do
        local ztype=$(cat "$(dirname "$zone")/type" 2>/dev/null)
        if [[ "$ztype" == *"pkg"* ]] || [[ "$ztype" == "TCPU" ]] || [[ "$ztype" == "k10temp" ]]; then
            local t=$(cat "$zone" 2>/dev/null)
            t=$((t / 1000))
            [ $t -gt $max_temp ] && max_temp=$t
        fi
    done
    [ $max_temp -eq 0 ] && max_temp=$(cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null | awk '{print int($1/1000)}')
    echo $max_temp
}

CPU_TEMP=$(get_cpu_temp)
[ -z "$CPU_TEMP" ] || [ "$CPU_TEMP" -eq 0 ] && CPU_TEMP=50

# Thermal zones optimized for keyboard comfort (~35C)
if [ $CPU_TEMP -lt 40 ]; then
    MAX_PERF=85; EPP="balance_performance"; TURBO=0; MODE="COOL"
elif [ $CPU_TEMP -lt 45 ]; then
    MAX_PERF=70; EPP="balance_performance"; TURBO=0; MODE="COMFORT"
elif [ $CPU_TEMP -lt 50 ]; then
    MAX_PERF=60; EPP="balance_power"; TURBO=0; MODE="OPTIMAL"
elif [ $CPU_TEMP -lt 55 ]; then
    MAX_PERF=50; EPP="balance_power"; TURBO=1; MODE="WARM"
elif [ $CPU_TEMP -lt 60 ]; then
    MAX_PERF=40; EPP="power"; TURBO=1; MODE="HOT"
else
    MAX_PERF=30; EPP="power"; TURBO=1; MODE="CRITICAL"
fi

# Apply settings based on driver
if [ "$DRIVER" = "intel_pstate" ]; then
    echo $MAX_PERF > /sys/devices/system/cpu/intel_pstate/max_perf_pct 2>/dev/null
    echo 10 > /sys/devices/system/cpu/intel_pstate/min_perf_pct 2>/dev/null
    echo $TURBO > /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null
elif [ "$DRIVER" = "amd_pstate" ]; then
    echo $MAX_PERF > /sys/devices/system/cpu/amd_pstate/max_perf_pct 2>/dev/null
    [ $TURBO -eq 1 ] && echo 0 > /sys/devices/system/cpu/boost 2>/dev/null
    [ $TURBO -eq 0 ] && echo 1 > /sys/devices/system/cpu/boost 2>/dev/null
else
    max_freq=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq 2>/dev/null || echo 4400000)
    target=$((max_freq * MAX_PERF / 100))
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_max_freq; do
        echo $target > "$cpu" 2>/dev/null
    done
fi

for epp in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
    echo $EPP > "$epp" 2>/dev/null
done

# Keyboard temperature estimate
KEYBOARD_EST=$((28 + (CPU_TEMP - 28) * 45 / 100))

# Log
LOG_FILE="/var/log/thermal-manager.log"
TIMESTAMP=$(date '+%H:%M:%S')
echo "$TIMESTAMP | CPU:${CPU_TEMP}C | Kbd:~${KEYBOARD_EST}C | ${MAX_PERF}% | $MODE" >> $LOG_FILE 2>/dev/null

# Keep log small
[ $(wc -l < $LOG_FILE 2>/dev/null || echo 0) -gt 2000 ] && tail -1000 $LOG_FILE > ${LOG_FILE}.tmp && mv ${LOG_FILE}.tmp $LOG_FILE

echo "comfort-$MODE" > /tmp/cpu-mode.current 2>/dev/null
