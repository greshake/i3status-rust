#!/bin/bash

RES=`curl -I $1 | head -n1 | cut -d' ' -f2`

case $RES in
1*)
    STATE="Info";;
2*)
    STATE="Good";;
3*)
    STATE="Warning";;
*)
    STATE="Critical";;
esac

echo "{\"icon\":\"cogs\",\"state\":\"${STATE}\",\"text\":\"${RES}\"}"
