#!/usr/bin/env zsh
# Build the UFO site and deploy it to AWS (S3 + CloudFront, Pulumi project in site/infra),
# then clear CloudFront's cache so the new files show at once.
#
#   cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev
#   cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev --domain ufoscene.net
#   cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev --name-servers-set
#
# --domain <name>     put the site on <name> and www.<name>, with DNS in Route 53:
#                     - a public hosted zone for <name> in this account (Route 53 makes one when it
#                       registers a domain, see register-domain.sh) is used as it is; otherwise
#                       Pulumi creates the zone;
#                     - if <name> is registered in Route 53 in this account, Pulumi sets the
#                       registration's name servers to the zone's, so DNS and the certificate
#                       are done in one run;
#                     - registered elsewhere: the name servers to set there are printed at the end;
#                       after setting them, run again with --name-servers-set.
# --name-servers-set  the registrar now uses the zone's name servers: issue the certificate and
#                     point the domain at CloudFront.
# Other arguments go to `pulumi up` (for example --yes). The stack must exist already
# (see site/README.md). Use your own AWS profile name.
set -euo pipefail
SITE="${0:A:h}"

usage() {
  print -u2 "usage: AWS_PROFILE=<your-profile> ./deploy.sh <stack> [--domain <name>] [--name-servers-set] [pulumi up options]"
  exit 1
}
[[ $# -ge 1 && "$1" != -* ]] || usage
STACK="$1"
shift
DOMAIN=""
NS_SET=false
UP_ARGS=()
while (( $# )); do
  case "$1" in
    --domain) [[ $# -ge 2 ]] || usage; DOMAIN="${2:l}"; shift 2 ;;
    --domain=*) DOMAIN="${${1#*=}:l}"; shift ;;
    --name-servers-set) NS_SET=true; shift ;;
    *) UP_ARGS+=("$1"); shift ;;
  esac
done
if [[ -n "$DOMAIN" && ! "$DOMAIN" =~ '^([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$' ]]; then
  print -u2 "deploy.sh: '$DOMAIN' does not look like a domain name (example: ufoscene.net)"
  exit 1
fi
[[ -n "${AWS_PROFILE:-}" ]] || print -u2 "deploy.sh: AWS_PROFILE is not set, using the default AWS credentials"

"$SITE/build.sh"

cd "$SITE/infra"
[[ -d node_modules ]] || npm install --no-audit --no-fund

cfg() { pulumi config get "$1" -s "$STACK" 2>/dev/null || true; }
set_cfg() { pulumi config set "$1" "$2" -s "$STACK"; }
rm_cfg() { pulumi config rm "$1" -s "$STACK" >/dev/null 2>&1 || true; }

if [[ -n "$DOMAIN" ]]; then
  if [[ "$(cfg ufo:domain)" == "$DOMAIN" && "$(cfg ufo:createZone)" == true ]]; then
    print "DNS: $DOMAIN keeps the hosted zone this stack created"
  else
    # Route 53 Domains creates a public zone when it registers a domain: use that one if it exists
    EXISTING="$(aws route53 list-hosted-zones-by-name --dns-name "$DOMAIN" --max-items 1 \
      --query "HostedZones[?Name=='$DOMAIN.' && Config.PrivateZone==\`false\`].Id | [0]" --output text)"
    rm_cfg ufo:hostedZoneId
    rm_cfg ufo:certificateReady # a new zone must answer before the certificate can validate
    if [[ -n "$EXISTING" && "$EXISTING" != None ]]; then
      print "DNS: using the existing hosted zone ${EXISTING#/hostedzone/} for $DOMAIN"
      set_cfg ufo:route53 true
      set_cfg ufo:createZone false
    else
      print "DNS: Pulumi creates a hosted zone for $DOMAIN"
      set_cfg ufo:createZone true
      set_cfg ufo:route53 false
    fi
  fi
  set_cfg ufo:domain "$DOMAIN"
  if aws route53domains get-domain-detail --region us-east-1 --domain-name "$DOMAIN" >/dev/null 2>&1; then
    print "DNS: $DOMAIN is registered in Route 53 in this account: its name servers will point at the zone"
    set_cfg ufo:fixNameServers true
  else
    print "DNS: $DOMAIN is not registered in Route 53 in this account: set the name servers printed at the end at its registrar"
    set_cfg ufo:fixNameServers false
  fi
fi
if $NS_SET; then
  set_cfg ufo:certificateReady true
fi

pulumi up -s "$STACK" "${UP_ARGS[@]}"

DIST_ID="$(pulumi stack output distributionId -s "$STACK")"
print "Invalidating CloudFront cache of $DIST_ID …"
aws cloudfront create-invalidation --distribution-id "$DIST_ID" --paths '/*' --query 'Invalidation.Id' --output text

print "\nSite:                  $(pulumi stack output url -s "$STACK")"
print "Spotify redirect URI:  $(pulumi stack output spotifyRedirectUri -s "$STACK")  (add it in the Spotify dashboard)"
NEXT="$(pulumi stack output nextStep -s "$STACK" 2>/dev/null || true)"
if [[ -n "$NEXT" ]]; then
  print "\nNext: $NEXT"
fi
