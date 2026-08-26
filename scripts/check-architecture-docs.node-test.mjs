import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

import {
  affectedValidationSlices,
  parseJsonFrontMatterText,
  validateFeatureDossiers,
  validatePostmortems,
  validateProposalReferences,
  validateProposals,
} from "./check-architecture-docs.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(readFileSync(path.join(root, "docs/features/feature-dossier.schema.json"), "utf8"));
const headings = [
  "用户目标",
  "验收场景",
  "明确不规定的实现",
  "局部假设",
  "架构决策",
  "当前实现入口",
  "验证状态",
  "澄清历史",
];

function validator() {
  const ajv = new Ajv2020({ allErrors: true, strict: true, formats: { date: /^\d{4}-\d{2}-\d{2}$/ } });
  return ajv.compile(schema);
}

function metadata(overrides = {}) {
  return {
    schemaVersion: 1,
    featureId: "FEAT-TEST",
    specStatus: "confirmed",
    implementationStatus: "implemented",
    validationStatus: "partial",
    components: ["frontend.features"],
    decisions: ["ADR-0010"],
    validationSlices: [{ id: "AC-TEST-01", components: ["frontend.features"] }],
    evidence: [],
    ...overrides,
  };
}

function dossier(overrides = {}) {
  return {
    file: path.join(root, "docs/features/test.md"),
    metadata: metadata(overrides),
    body: `${headings.map((heading) => `## ${heading}`).join("\n\n")}\n\nAC-TEST-01`,
  };
}

function digestFeatureFiles() {
  const files = ["README.md", "incident-vault.md", "shortcut-binding.md"];
  const hash = createHash("sha256");
  for (const file of files) hash.update(readFileSync(path.join(root, "docs/features", file)));
  return hash.digest("hex");
}

test("JSON front matter parses deterministically", () => {
  const parsed = parseJsonFrontMatterText('---\n{"featureId":"FEAT-TEST"}\n---\nbody', "fixture");
  assert.equal(parsed.metadata.featureId, "FEAT-TEST");
  assert.equal(parsed.body, "body");
  assert.throws(() => parseJsonFrontMatterText("# no metadata", "fixture"), /缺少 JSON front matter/);
});

test("valid dossier passes and invalid status is rejected", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier()],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.deepEqual(errors, []);

  const invalidErrors = [];
  validateFeatureDossiers(
    [dossier({ validationStatus: "finished" })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    invalidErrors,
  );
  assert.ok(invalidErrors.some((error) => error.includes("validationStatus")));
});

test("unknown component and ADR references are rejected", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({ components: ["unknown.component"], decisions: ["ADR-9999"] })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("未知组件")));
  assert.ok(errors.some((error) => error.includes("未知 ADR")));
});

test("validated dossier requires current passing evidence for every slice", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({
      validationStatus: "validated",
      evidence: [{
        id: "EV-TEST-01",
        acceptanceIds: ["AC-TEST-01"],
        method: "automated",
        result: "pass",
        freshness: "stale",
        sourceRevision: "7026768",
        worktreeState: "dirty",
        changedPaths: ["scripts/check-architecture-docs.test.mjs"],
        environment: "Node test fixture",
        validatedAt: "2026-08-26",
      }],
    })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("缺少当前成功证据")));
});

test("proposal references and postmortem normativity are checked", () => {
  const referenceErrors = [];
  validateProposalReferences(
    { components: [{ id: "frontend.features", docs: ["docs/architecture/proposals/test.md"], adrs: [] }] },
    "docs/architecture/proposals/",
    referenceErrors,
  );
  assert.equal(referenceErrors.length, 1);

  const directory = mkdtempSync(path.join(tmpdir(), "gy-doc-governance-"));
  try {
    const proposalDir = path.join(directory, "proposals");
    const postmortemDir = path.join(directory, "postmortems");
    writeFileSync(path.join(directory, "placeholder"), "", "utf8");
    mkdirSync(proposalDir);
    mkdirSync(postmortemDir);
    writeFileSync(
      path.join(proposalDir, "bad.md"),
      '---\n{"documentType":"architecture-proposal","viewStatus":"current","owner":"team","createdAt":"2026-08-26","revisitWhen":"probe","relatedFeatures":["FEAT-UNKNOWN"]}\n---\n',
      "utf8",
    );
    writeFileSync(
      path.join(postmortemDir, "bad.md"),
      '---\n{"documentType":"postmortem","normative":true,"incidentDate":"2026-08-26","affectedRevisions":["abcdef0"]}\n---\n',
      "utf8",
    );
    const errors = [];
    validateProposals(proposalDir, new Set(["FEAT-TEST"]), errors);
    validatePostmortems(postmortemDir, errors);
    assert.ok(errors.some((error) => error.includes("viewStatus=proposed")));
    assert.ok(errors.some((error) => error.includes("未知 Feature")));
    assert.ok(errors.some((error) => error.includes("normative=false")));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("component impact maps to validation slices", () => {
  const affected = affectedValidationSlices(
    [dossier()],
    new Set(["frontend.features"]),
  );
  assert.deepEqual(affected, [{ featureId: "FEAT-TEST", sliceId: "AC-TEST-01" }]);
});

test("impact report is warning-only and does not rewrite dossiers", () => {
  const before = digestFeatureFiles();
  const result = spawnSync(process.execPath, ["scripts/check-architecture-docs.mjs", "--impact"], {
    cwd: root,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Potentially Stale/);
  assert.equal(digestFeatureFiles(), before);
});
