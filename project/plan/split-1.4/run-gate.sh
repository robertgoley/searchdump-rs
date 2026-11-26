#!/bin/bash
# run-gate.sh LOGNAME [args]: detached gate; poll for ^rc= in /tmp/LOGNAME.log
L=/tmp/$1.log; shift
rm -f "$L"
nohup setsid bash -c "/workspace/rel-1.4/project/plan/split-1.4/gate-fast.sh $* > $L 2>&1 < /dev/null; echo rc=\$? >> $L" > /dev/null 2>&1 &
echo "started $L"
