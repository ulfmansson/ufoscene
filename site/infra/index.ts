// UFO demo website on AWS: a private S3 bucket that only CloudFront can read (Origin Access
// Control), a CloudFront distribution in front of it, and optionally a custom domain with an
// ACM certificate (us-east-1) and Route 53 records. The files come from ../dist, built by
// ../build.sh; Pulumi uploads them, so a changed file is re-uploaded on the next `pulumi up`.

import * as aws from "@pulumi/aws";
import * as pulumi from "@pulumi/pulumi";
import * as crypto from "crypto";
import * as fs from "fs";
import * as path from "path";

const config = new pulumi.Config("ufo");
const domain = (config.get("domain") || "").trim().toLowerCase();
const certificateReady = config.getBoolean("certificateReady") ?? false;
// www.<domain> as well, sent on to the bare domain (the Spotify redirect URI names one host only)
const www = domain !== "" && (config.getBoolean("www") ?? true);
const names = domain ? (www ? [domain, `www.${domain}`] : [domain]) : [];

// The domain's Route 53 hosted zone, one of:
//  - ufo:createZone=true   Pulumi creates it (and deletes it again with `pulumi destroy`);
//  - ufo:hostedZoneId      an existing zone, by ID;
//  - ufo:route53=true      an existing zone, looked up by name (registering a domain in
//                          Route 53 creates one, see ../register-domain.sh).
const createZone = domain !== "" && (config.getBoolean("createZone") ?? false);
const givenZoneId = domain ? config.get("hostedZoneId") || "" : "";
const lookUpZone = domain !== "" && !givenZoneId && (config.getBoolean("route53") ?? false);
if (createZone && (givenZoneId || lookUpZone)) {
    throw new Error("ufo:createZone cannot be combined with ufo:hostedZoneId or ufo:route53: pick one");
}
// ufo:fixNameServers=true: the domain is registered in Route 53 in this account, and its
// registration should use this zone's name servers (needed after the zone is created anew).
const fixNameServers = domain !== "" && (config.getBoolean("fixNameServers") ?? false);
const maxAge = config.getNumber("maxAge") ?? 60;
const priceClass = config.get("priceClass") || "PriceClass_100";
// ufo:googleSiteVerification: the token Google Search Console gives for a "Domain" property
// (the part after google-site-verification=). Needs the Route 53 zone; adds the TXT record.
const googleSiteVerification = (config.get("googleSiteVerification") || "").trim().replace(/^google-site-verification=/, "");

const distDir = path.resolve(__dirname, "..", "dist");
if (!fs.existsSync(path.join(distDir, "index.html"))) {
    throw new Error(`${distDir}/index.html is missing: run site/build.sh first`);
}

const tags = { project: "ufo", stack: pulumi.getStack() };

// ------------------------------------------------------------------ bucket (private)

const bucket = new aws.s3.Bucket("site", { forceDestroy: true, tags });

new aws.s3.BucketPublicAccessBlock("site", {
    bucket: bucket.id,
    blockPublicAcls: true,
    blockPublicPolicy: true,
    ignorePublicAcls: true,
    restrictPublicBuckets: true,
});

new aws.s3.BucketOwnershipControls("site", {
    bucket: bucket.id,
    rule: { objectOwnership: "BucketOwnerEnforced" },
});

// ------------------------------------------------------------------ files from ../dist

const contentTypes: Record<string, string> = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".wasm": "application/wasm",
    ".txt": "text/plain; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".xml": "application/xml; charset=utf-8",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".svg": "image/svg+xml",
    ".ico": "image/x-icon",
};

// Not for S3: dot-files (.nojekyll), and _headers (CloudFront gets its headers below).
const skip = (name: string) => name.startsWith(".") || name === "_headers";

function listFiles(dir: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        if (skip(e.name)) return [];
        const full = path.join(dir, e.name);
        return e.isDirectory() ? listFiles(full) : [full];
    });
}

const objects = listFiles(distDir).map((file) => {
    const key = path.relative(distDir, file).split(path.sep).join("/");
    const contentType = contentTypes[path.extname(file).toLowerCase()];
    if (!contentType) throw new Error(`no content type for ${key}: add its extension to contentTypes in index.ts`);
    const md5 = crypto.createHash("md5").update(fs.readFileSync(file)).digest("hex");
    return new aws.s3.BucketObjectv2(`file:${key}`, {
        bucket: bucket.id,
        key,
        source: new pulumi.asset.FileAsset(file),
        contentType,
        // file names are not hashed, so nobody may keep a file for long
        cacheControl: `public, max-age=${maxAge}, must-revalidate`,
        sourceHash: md5, // a changed file is uploaded again
        tags,
    });
});

// ------------------------------------------------------------------ DNS zone (optional)

// CloudFront only takes certificates from us-east-1, and Route 53 Domains only answers there.
const useEast1 = new aws.Provider("us-east-1", { region: "us-east-1" });

const zone = createZone
    ? new aws.route53.Zone("site", { name: domain, comment: `UFO site (${pulumi.getStack()}), managed by Pulumi`, tags })
    : undefined;
const existingZone = givenZoneId
    ? aws.route53.getZoneOutput({ zoneId: givenZoneId })
    : lookUpZone
      ? aws.route53.getZoneOutput({ name: domain, privateZone: false })
      : undefined;
const zoneId: pulumi.Input<string> | undefined = zone ? zone.zoneId : existingZone?.zoneId;
const zoneNameServers: pulumi.Output<string[]> | undefined = zone ? zone.nameServers : existingZone?.nameServers;

// Point the Route 53 registration at the zone. This resource only changes the registration's
// settings: it never buys, renews or deletes the domain (`pulumi destroy` just forgets it).
// The provider's defaults keep auto-renew, transfer lock and WHOIS privacy on.
let nameServerFix: aws.route53domains.RegisteredDomain | undefined;
if (fixNameServers) {
    if (!zoneNameServers) throw new Error("ufo:fixNameServers needs a zone: ufo:createZone, ufo:route53 or ufo:hostedZoneId");
    nameServerFix = new aws.route53domains.RegisteredDomain(
        "site",
        { domainName: domain, nameServers: zoneNameServers.apply((ns) => ns.map((name) => ({ name }))), tags },
        { provider: useEast1 },
    );
}

// Can ACM read the validation records yet? A zone Pulumi just created only answers once the
// registration uses its name servers: done here with ufo:fixNameServers, or by hand at another
// registrar (then set ufo:certificateReady=true, see the output nextStep).
const zoneAnswers = !createZone || fixNameServers || certificateReady;

// ------------------------------------------------------------------ certificate (optional)

let certificateArn: pulumi.Output<string> | undefined;
let validationRecords: pulumi.Output<{ name: string; type: string; value: string }[]> | undefined;

if (domain) {
    const cert = new aws.acm.Certificate(
        "site",
        { domainName: domain, subjectAlternativeNames: names.slice(1), validationMethod: "DNS", tags },
        { provider: useEast1 },
    );
    validationRecords = cert.domainValidationOptions.apply((options) =>
        options.map((o) => ({
            name: o.resourceRecordName ?? "",
            type: o.resourceRecordType ?? "CNAME",
            value: o.resourceRecordValue ?? "",
        })),
    );

    if (zoneId) {
        // one CNAME per name on the certificate (ACM may give apex and www the same one: allowOverwrite)
        const records = names.map((_, i) => {
            const dvo = cert.domainValidationOptions[i];
            return new aws.route53.Record(i === 0 ? "cert-validation" : `cert-validation-${i}`, {
                zoneId,
                name: dvo.resourceRecordName.apply((n) => n ?? ""),
                type: dvo.resourceRecordType.apply((t) => t ?? "CNAME"),
                records: [dvo.resourceRecordValue.apply((v) => v ?? "")],
                ttl: 300,
                allowOverwrite: true,
            });
        });
        if (zoneAnswers) {
            certificateArn = new aws.acm.CertificateValidation(
                "site",
                { certificateArn: cert.arn, validationRecordFqdns: records.map((r) => r.fqdn) },
                {
                    provider: useEast1,
                    // after a name-server change the .com servers may need a while to pass it on
                    dependsOn: nameServerFix ? [nameServerFix] : [],
                    customTimeouts: { create: "2h" },
                },
            ).certificateArn;
        }
    } else if (certificateReady) {
        // DNS elsewhere: the validation CNAME was added by hand (see the output), so this just waits for "issued"
        certificateArn = new aws.acm.CertificateValidation(
            "site",
            { certificateArn: cert.arn },
            { provider: useEast1 },
        ).certificateArn;
    }
}

// ------------------------------------------------------------------ CloudFront

const oac = new aws.cloudfront.OriginAccessControl("site", {
    description: "UFO site bucket, read by CloudFront only",
    originAccessControlOriginType: "s3",
    signingBehavior: "always",
    signingProtocol: "sigv4",
});

// Spotify sends the browser back to /callback?code=…: serve the page itself there (status 200).
// www.<domain> answers with a 301 to the bare domain (the browser keeps the #fragment), so the
// page always builds the one redirect URI that is registered at Spotify.
const wwwRedirect =
    www && certificateArn
        ? `
  var host = request.headers.host ? request.headers.host.value.toLowerCase() : '';
  if (host === 'www.${domain}') {
    var qs = [];
    for (var key in request.querystring) {
      var p = request.querystring[key];
      var values = p.multiValue ? p.multiValue : [p];
      for (var i = 0; i < values.length; i++) qs.push(key + '=' + values[i].value);
    }
    var target = 'https://${domain}' + request.uri + (qs.length ? '?' + qs.join('&') : '');
    return { statusCode: 301, statusDescription: 'Moved Permanently', headers: { location: { value: target } } };
  }`
        : "";
const callbackRewrite = new aws.cloudfront.Function("callback-rewrite", {
    runtime: "cloudfront-js-2.0",
    comment: "UFO: /callback and /callback/ are the page itself (Spotify login); www -> bare domain",
    publish: true,
    code: `function handler(event) {
  var request = event.request;${wwwRedirect}
  if (request.uri === '/callback' || request.uri === '/callback/') {
    request.uri = '/index.html';
  }
  // a show's own address is the page too, which starts the show; with a slash the page would
  // look for its files one folder down, so that one moves to the address without it
  var show = request.uri.toLowerCase().match(/^\\/(kite|queen|concert|ukraine)(\\/?)$/);
  if (show && show[2]) {
    return { statusCode: 301, statusDescription: 'Moved Permanently', headers: { location: { value: '/' + show[1] } } };
  }
  if (show) {
    request.uri = '/index.html';
  }
  return request;
}
`,
});

const cachePolicy = new aws.cloudfront.CachePolicy("site", {
    comment: "UFO: short caching, files are not hashed (the objects send max-age)",
    minTtl: 0,
    defaultTtl: maxAge,
    maxTtl: Math.max(maxAge, 300),
    parametersInCacheKeyAndForwardedToOrigin: {
        enableAcceptEncodingGzip: true,
        enableAcceptEncodingBrotli: true,
        cookiesConfig: { cookieBehavior: "none" },
        headersConfig: { headerBehavior: "none" },
        queryStringsConfig: { queryStringBehavior: "none" },
    },
});

const headersPolicy = new aws.cloudfront.ResponseHeadersPolicy("site", {
    comment: "UFO: security headers; microphone and screen/tab capture for the page itself",
    securityHeadersConfig: {
        contentTypeOptions: { override: true },
        frameOptions: { frameOption: "SAMEORIGIN", override: true },
        referrerPolicy: { referrerPolicy: "strict-origin-when-cross-origin", override: true },
        strictTransportSecurity: { accessControlMaxAgeSec: 31536000, includeSubdomains: false, override: true },
    },
    customHeadersConfig: {
        items: [
            {
                header: "Permissions-Policy",
                value: "microphone=(self), display-capture=(self), camera=(), geolocation=()",
                override: true,
            },
        ],
    },
});

const originId = "s3-site";
const distribution = new aws.cloudfront.Distribution(
    "site",
    {
        enabled: true,
        comment: `UFO demo site (${pulumi.getStack()})`,
        isIpv6Enabled: true,
        httpVersion: "http2and3",
        defaultRootObject: "index.html",
        priceClass,
        // S3 answers 403 for a file that does not exist (CloudFront may not list the bucket):
        // give visitors and search engines a real 404 instead.
        customErrorResponses: [403, 404].map((errorCode) => ({
            errorCode,
            responseCode: 404,
            responsePagePath: "/404.html",
            errorCachingMinTtl: 60,
        })),
        aliases: certificateArn ? names : [],
        origins: [
            {
                originId,
                domainName: bucket.bucketRegionalDomainName,
                originAccessControlId: oac.id,
            },
        ],
        defaultCacheBehavior: {
            targetOriginId: originId,
            viewerProtocolPolicy: "redirect-to-https",
            allowedMethods: ["GET", "HEAD", "OPTIONS"],
            cachedMethods: ["GET", "HEAD"],
            compress: true,
            cachePolicyId: cachePolicy.id,
            responseHeadersPolicyId: headersPolicy.id,
            functionAssociations: [{ eventType: "viewer-request", functionArn: callbackRewrite.arn }],
        },
        restrictions: { geoRestriction: { restrictionType: "none", locations: [] } },
        viewerCertificate: certificateArn
            ? { acmCertificateArn: certificateArn, sslSupportMethod: "sni-only", minimumProtocolVersion: "TLSv1.2_2021" }
            : { cloudfrontDefaultCertificate: true },
        tags,
    },
    { dependsOn: objects },
);

// Only this distribution may read the bucket.
new aws.s3.BucketPolicy("site", {
    bucket: bucket.id,
    policy: pulumi.all([bucket.arn, distribution.arn]).apply(([bucketArn, distributionArn]) =>
        JSON.stringify({
            Version: "2012-10-17",
            Statement: [
                {
                    Sid: "CloudFrontReadOnly",
                    Effect: "Allow",
                    Principal: { Service: "cloudfront.amazonaws.com" },
                    Action: "s3:GetObject",
                    Resource: `${bucketArn}/*`,
                    Condition: { StringEquals: { "AWS:SourceArn": distributionArn } },
                },
            ],
        }),
    ),
});

// ------------------------------------------------------------------ DNS (optional)

if (zoneId && certificateArn) {
    for (const name of names) {
        for (const type of ["A", "AAAA"]) {
            new aws.route53.Record(name === domain ? `alias-${type}` : `alias-www-${type}`, {
                zoneId,
                name,
                type,
                aliases: [{ name: distribution.domainName, zoneId: distribution.hostedZoneId, evaluateTargetHealth: false }],
            });
        }
    }
}

// Google Search Console, "Domain" property: proves ownership of the domain and every subdomain.
if (googleSiteVerification) {
    if (!zoneId) throw new Error("ufo:googleSiteVerification needs a Route 53 zone (ufo:route53, ufo:hostedZoneId or ufo:createZone)");
    new aws.route53.Record("google-site-verification", {
        zoneId,
        name: domain,
        type: "TXT",
        ttl: 300,
        records: [`google-site-verification=${googleSiteVerification}`],
    });
}

// ------------------------------------------------------------------ outputs

const host = certificateArn ? pulumi.output(domain) : distribution.domainName;
export const url = pulumi.interpolate`https://${host}/`;
export const spotifyRedirectUri = pulumi.interpolate`https://${host}/callback`;
export const cloudfrontUrl = pulumi.interpolate`https://${distribution.domainName}/`;
export const distributionId = distribution.id;
export const bucketName = bucket.bucket;
export const fileCount = objects.length;
export const hostedZoneId = zoneId;
export const nameServers = zoneNameServers;
// What is still missing before the domain works, for deploy.sh to print.
export const nextStep: pulumi.Output<string> | undefined =
    createZone && !zoneAnswers && zoneNameServers
        ? zoneNameServers.apply(
              (ns) =>
                  `Set these name servers for ${domain} at your registrar: ${ns.join(" ")} — ` +
                  `then deploy again with --name-servers-set`,
          )
        : undefined;
// Without a Route 53 zone: add these records at your DNS provider, wait for "Issued" in ACM
// (us-east-1), then `pulumi config set ufo:certificateReady true` and `pulumi up` again.
export const certificateValidationRecords =
    domain && !zoneId && !certificateReady ? validationRecords : undefined;
export const dnsToAdd =
    domain && !zoneId && certificateReady
        ? distribution.domainName.apply((target) =>
              names.map((name) => `${name}  CNAME (or ALIAS/flattened)  ${target}`),
          )
        : undefined;
