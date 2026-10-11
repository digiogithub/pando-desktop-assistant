#!/usr/bin/env bash
# Sweeps the paths spike on Xvfb. Usage: run-paths-sweep.sh <label> [env assignments...]
# Intel/AMD/Apple GPUs cannot be selected under Xvfb; run on a real session for those.
set -u
B=${B:-target/spikes/release/gpui-shell-spikes}
label=$1; shift
for cfg in "--scale 0" "--scale 1" "--scale 1 --mode cached" "--scale 1 --order interleaved" "--scale 2" "--scale 4" "--scale 4 --mode cached"; do
  env "$@" WAYLAND_DISPLAY= xvfb-run -a -s "-screen 0 1200x1200x24" timeout 200 "$B" paths --frames 240 $cfg 2>&1 | grep '^{' | sed "s/^{/{\"label\":\"$label\",\"cfg\":\"$cfg\",/"
done
