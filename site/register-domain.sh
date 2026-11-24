#!/usr/bin/env zsh
# Register a domain for the UFO site in Route 53 Domains. This is a purchase: it charges the
# AWS account the yearly price shown, renews every year (auto-renew on), and cannot be undone
# (Route 53 does not refund a registration). It asks you to type the domain before it buys.
#
#   cd site && AWS_PROFILE=<your-profile> ./register-domain.sh example.net
#   cd site && AWS_PROFILE=<your-profile> ./register-domain.sh --status <operation-id>
#
# Contact details come from site/domain-contact.json (copy domain-contact.example.json; the
# file is git-ignored). WHOIS privacy is switched on for every contact. Route 53 creates the
# domain's public hosted zone itself; site/infra finds it with `ufo:route53 true`.
# Kept outside Pulumi on purpose: `pulumi destroy` must never be able to delete the domain.
set -euo pipefail
SITE="${0:A:h}"
REGION=us-east-1 # Route 53 Domains only answers in us-east-1
r53d() { aws route53domains --region "$REGION" "$@"; }

if [[ "${1:-}" == --status ]]; then
  [[ -n "${2:-}" ]] || { print -u2 "usage: ./register-domain.sh --status <operation-id>"; exit 1; }
  r53d get-operation-detail --operation-id "$2" --query '{Status: Status, Domain: DomainName, Message: Message}' --output table
  exit 0
fi

if [[ $# -ne 1 || "$1" == -* ]]; then
  print -u2 "usage: AWS_PROFILE=<profile> ./register-domain.sh <domain>"
  exit 1
fi
DOMAIN="${1:l}"
if [[ ! "$DOMAIN" =~ '^[a-z0-9]([a-z0-9-]*[a-z0-9])?\.[a-z]{2,}$' ]]; then
  print -u2 "register-domain.sh: '$DOMAIN' should be a second-level name such as example.net"
  exit 1
fi
TLD="${DOMAIN#*.}"

CONTACT="$SITE/domain-contact.json"
if [[ ! -f "$CONTACT" ]]; then
  print -u2 "register-domain.sh: $CONTACT is missing; copy domain-contact.example.json and fill it in"
  exit 1
fi
python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$CONTACT" || { print -u2 "register-domain.sh: $CONTACT is not valid JSON"; exit 1; }

print "Checking $DOMAIN …"
AVAILABILITY="$(r53d check-domain-availability --domain-name "$DOMAIN" --query Availability --output text)"
if [[ "$AVAILABILITY" != AVAILABLE ]]; then
  print -u2 "register-domain.sh: $DOMAIN is $AVAILABILITY, not AVAILABLE"
  exit 1
fi
PRICE="$(r53d list-prices --tld "$TLD" --query 'Prices[0].RegistrationPrice.[Price,Currency]' --output text | tr '\t' ' ')"
RENEW="$(r53d list-prices --tld "$TLD" --query 'Prices[0].RenewalPrice.[Price,Currency]' --output text | tr '\t' ' ')"
ACCOUNT="$(aws sts get-caller-identity --query Account --output text)"
EMAIL="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("Email",""))' "$CONTACT")"

print "
  Domain:        $DOMAIN (available)
  AWS account:   $ACCOUNT (profile ${AWS_PROFILE:-default})
  Price:         $PRICE for 1 year, then $RENEW a year (auto-renew on)
  Contacts:      $CONTACT, privacy protection on
  Verification:  ICANN mails $EMAIL; click the link within 15 days or the domain is suspended
  Hosted zone:   created by Route 53 (US\$0.50 a month)

  The purchase cannot be undone or refunded."
read -r "TYPED?Type the domain name to buy it: "
if [[ "$TYPED" != "$DOMAIN" ]]; then
  print "Not registered."
  exit 1
fi

OPERATION="$(r53d register-domain \
  --domain-name "$DOMAIN" \
  --duration-in-years 1 \
  --auto-renew \
  --admin-contact "file://$CONTACT" \
  --registrant-contact "file://$CONTACT" \
  --tech-contact "file://$CONTACT" \
  --privacy-protect-admin-contact \
  --privacy-protect-registrant-contact \
  --privacy-protect-tech-contact \
  --query OperationId --output text)"

print "
Registration started (operation $OPERATION). It usually takes 15 minutes to a few hours.
Check it:       cd $SITE && AWS_PROFILE=${AWS_PROFILE:-<your-profile>} ./register-domain.sh --status $OPERATION
When it says SUCCESSFUL, put the site on it (uses the hosted zone Route 53 just created):
  cd $SITE && AWS_PROFILE=${AWS_PROFILE:-<your-profile>} ./deploy.sh <stack> --domain $DOMAIN"
