// Sets "issued" in a server-info file to the current UTC time, to the second, just before it is signed.
import { readFileSync, writeFileSync } from "node:fs";

const file = process.argv[2];
const info = JSON.parse(readFileSync(file, "utf8"));
info.issued = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
writeFileSync(file, JSON.stringify(info, null, 2) + "\n");
console.log(`issued = ${info.issued}`);
