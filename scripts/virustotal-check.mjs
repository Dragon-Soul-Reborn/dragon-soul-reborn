// Uploads each file to VirusTotal, waits for the analysis, and exits 1 if any engine flags it.
// Free API: 4 requests a minute, so it polls every 20 seconds.
import { readFileSync } from "node:fs";
import { basename } from "node:path";

const key = process.env.VT_API_KEY;
if (!key) {
  console.error("VT_API_KEY is not set");
  process.exit(1);
}
const api = "https://www.virustotal.com/api/v3";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function check(path) {
  const form = new FormData();
  form.append("file", new Blob([readFileSync(path)]), basename(path));
  const upload = await fetch(`${api}/files`, { method: "POST", headers: { "x-apikey": key }, body: form });
  if (!upload.ok) throw new Error(`upload failed: HTTP ${upload.status} ${await upload.text()}`);
  const id = (await upload.json()).data.id;
  for (let i = 0; i < 60; i++) {
    await sleep(20_000);
    const r = await fetch(`${api}/analyses/${id}`, { headers: { "x-apikey": key } });
    if (!r.ok) continue;
    const a = (await r.json()).data.attributes;
    if (a.status !== "completed") continue;
    const flagged = Object.entries(a.results).filter(([, v]) => v.category === "malicious" || v.category === "suspicious");
    console.log(`${basename(path)}: ${a.stats.malicious} malicious, ${a.stats.suspicious} suspicious, ${a.stats.undetected} undetected`);
    for (const [engine, v] of flagged) console.log(`  ${engine}: ${v.category} (${v.result})`);
    return flagged.length === 0;
  }
  throw new Error("the VirusTotal analysis did not finish within 20 minutes");
}

let clean = true;
for (const file of process.argv.slice(2)) clean = (await check(file)) && clean;
process.exit(clean ? 0 : 1);
