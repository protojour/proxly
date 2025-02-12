#!/bin/sh
set -x

echo `whoami`

## ingress

# redirect common chain
iptables -t nat -N PROXLY_INGRESS

# skip ports
# iptables -t nat -A PROXLY_INGRESS -p tcp --match multiport --dports <ports> -j RETURN

# send packets to proxy ingress port
iptables -t nat -A PROXLY_INGRESS -p tcp -j REDIRECT --to-port 4645

# use PROXY_INIT_REDIRECT
iptables -t nat -A PREROUTING -j PROXLY_INGRESS


## egress
iptables -t nat -N PROXLY_EGRESS

# ignore proxly uid
iptables -t nat -A PROXLY_EGRESS -m owner --uid-owner 7855 -j RETURN

# ignore loopback
iptables -t nat -A PROXLY_EGRESS -o lo -j RETURN

# skip ports
# iptables -t nat -A PROXY_INIT_EGRESS -p tcp --match multiport --dports <ports> -j RETURN

# reroute to proxly's egress port
iptables -t nat -A PROXLY_EGRESS -p tcp -j REDIRECT --to-port 4647

# configure OUTPUT chain to use PROXLY_ERGRESS chain
iptables -t nat -A OUTPUT -j PROXLY_EGRESS
