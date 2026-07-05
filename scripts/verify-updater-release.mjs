#!/usr/bin/env node

const DEFAULT_REPO = "pelogvc/oorouter";

function parseArgs(argv) {
  const args = {
    repo: DEFAULT_REPO,
    tag: "latest",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--repo") {
      args.repo = argv[index + 1];
      index += 1;
    } else if (value === "--tag") {
      args.tag = argv[index + 1];
      index += 1;
    } else if (!value.startsWith("--")) {
      args.tag = value;
    }
  }

  if (!/^[^/]+\/[^/]+$/.test(args.repo)) {
    throw new Error(
      "Usage: node scripts/verify-updater-release.mjs [--repo owner/repo] [--tag v1.2.3|latest]"
    );
  }
  if (!args.tag) {
    throw new Error("Missing release tag");
  }

  return args;
}

function githubHeaders() {
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  return {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

async function fetchJson(url) {
  const response = await fetch(url, { headers: githubHeaders() });
  if (!response.ok) {
    throw new Error(`${url} returned HTTP ${response.status}`);
  }
  return response.json();
}

function releaseApiUrl(repo, tag) {
  if (tag === "latest") {
    return `https://api.github.com/repos/${repo}/releases/latest`;
  }
  return `https://api.github.com/repos/${repo}/releases/tags/${encodeURIComponent(tag)}`;
}

function check(name, passed, details, failures) {
  const marker = passed ? "PASS" : "FAIL";
  console.log(`${marker} ${name}${details ? ` - ${details}` : ""}`);
  if (!passed) {
    failures.push(name);
  }
}

function assetDownloadUrl(asset) {
  if (!asset.browser_download_url) {
    throw new Error(`Asset ${asset.name} is missing browser_download_url`);
  }
  return asset.browser_download_url;
}

function assetMetadataUrl(asset) {
  if (!asset.url) {
    throw new Error(`Asset ${asset.name} is missing API asset URL`);
  }
  return asset.url;
}

function assetUrlMatches(asset, url) {
  return url === assetDownloadUrl(asset) || url === assetMetadataUrl(asset);
}

function assetNames(assets) {
  return assets.map((asset) => asset.name).join(", ");
}

function singleMatchingAsset(assets, pattern) {
  const matches = assets.filter((asset) => pattern.test(asset.name));
  return {
    asset: matches.length === 1 ? matches[0] : undefined,
    count: matches.length,
    names: assetNames(matches),
  };
}

async function main() {
  const { repo, tag } = parseArgs(process.argv.slice(2));
  const release = await fetchJson(releaseApiUrl(repo, tag));
  const assets = Array.isArray(release.assets) ? release.assets : [];
  const failures = [];

  console.log(`Release: ${repo}@${release.tag_name}`);

  const dmgMatch = singleMatchingAsset(assets, /_aarch64\.dmg$/i);
  const updaterArchiveMatch = singleMatchingAsset(
    assets,
    /_aarch64\.app\.tar\.gz$/i
  );
  const dmg = dmgMatch.asset;
  const updaterArchive = updaterArchiveMatch.asset;
  const updaterSignature = updaterArchive
    ? assets.find((asset) => asset.name === `${updaterArchive.name}.sig`)
    : undefined;
  const latestJsonAsset = assets.find((asset) => asset.name === "latest.json");

  check(
    "exactly one Apple Silicon DMG asset",
    Boolean(dmg),
    dmg?.name ??
      `${dmgMatch.count} matches${dmgMatch.names ? `: ${dmgMatch.names}` : ""}`,
    failures
  );
  check(
    "exactly one updater .app.tar.gz asset",
    Boolean(updaterArchive),
    updaterArchive?.name ??
      `${updaterArchiveMatch.count} matches${updaterArchiveMatch.names ? `: ${updaterArchiveMatch.names}` : ""}`,
    failures
  );
  check("Updater signature asset", Boolean(updaterSignature), updaterSignature?.name, failures);
  check("latest.json asset", Boolean(latestJsonAsset), latestJsonAsset?.name, failures);

  if (latestJsonAsset) {
    const latestJson = await fetchJson(assetDownloadUrl(latestJsonAsset));
    const platform = latestJson?.platforms?.["darwin-aarch64"];
    const platformUrl = typeof platform?.url === "string" ? platform.url : "";
    const platformSignature =
      typeof platform?.signature === "string" ? platform.signature.trim() : "";
    const updaterArchiveUrlMatches = updaterArchive
      ? assetUrlMatches(updaterArchive, platformUrl)
      : false;
    const metadataVersion = typeof latestJson?.version === "string" ? latestJson.version : "";
    const releaseVersion =
      typeof release.tag_name === "string" ? release.tag_name.replace(/^v/, "") : "";
    const normalizedMetadataVersion = metadataVersion.replace(/^v/, "");

    check("latest.json darwin-aarch64 platform", Boolean(platform), "", failures);
    check(
      "latest.json version matches release tag",
      normalizedMetadataVersion === releaseVersion,
      metadataVersion,
      failures
    );
    check(
      "latest.json URL points to this release updater archive",
      updaterArchiveUrlMatches,
      platformUrl,
      failures
    );
    check("latest.json contains inline signature", platformSignature.length > 0, "", failures);

    if (updaterSignature) {
      const updaterSignatureUrl = assetDownloadUrl(updaterSignature);
      const signatureResponse = await fetch(updaterSignatureUrl, {
        headers: githubHeaders(),
      });
      if (!signatureResponse.ok) {
        throw new Error(`${updaterSignatureUrl} returned HTTP ${signatureResponse.status}`);
      }
      const assetSignature = (await signatureResponse.text()).trim();
      check(
        "latest.json signature matches updater signature asset",
        platformSignature === assetSignature,
        "",
        failures
      );
    }
  }

  if (failures.length > 0) {
    console.error(`Updater release verification failed: ${failures.join(", ")}`);
    process.exit(1);
  }

  console.log("Updater release verification passed.");
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
