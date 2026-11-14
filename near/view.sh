#!/bin/sh
# Read the live contract. No account needed.
#   ./view.sh state
#   ./view.sh check_mint '{"coins":"1500000"}'
C=coin.prusd-mwgt5ryd.testnet; ARGS="${2:-}"; [ -z "$ARGS" ] && ARGS='{}'
A=$(printf '%s' "$ARGS" | base64 | tr -d '\n')
curl -s https://test.rpc.fastnear.com -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"query","params":{"request_type":"call_function","finality":"final","account_id":"'$C'","method_name":"'$1'","args_base64":"'$A'"}}' \
 | python3 -c "import json,sys;r=json.load(sys.stdin);print(bytes(r['result']['result']).decode() if 'result' in r.get('result',{}) else r)"
