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
  effectiveSliceFreshness,
  affectedValidationSlices,
  collectCohesionWarnings,
  parseJsonFrontMatterText,
  validateCurrentViews,
  validateFeatureDossiers,
  validateImplementationClaims,
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
    schemaVersion: 3,
    featureId: "FEAT-TEST",
    specStatus: "confirmed",
    confirmation: {
      confirmedBy: "test user",
      confirmedAt: "2026-08-26",
      sourceRef: "test fixture",
    },
    implementationStatus: "implemented",
    implementationReview: {
      status: "conformant",
      sourceRevision: "7026768",
      worktreeState: "clean",
      reviewedAt: "2026-08-27",
      summary: "Test fixture implementation review",
      knownDeviations: [],
    },
    validationStatus: "partial",
    components: ["frontend.features"],
    decisions: ["ADR-0010"],
    validationSlices: [{ id: "AC-TEST-01", components: ["frontend.features"], requiredEvidence: ["automated"] }],
    evidence: [],
    impactAssessments: [],
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

test("confirmed dossier requires a traceable confirmation source", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({ confirmation: undefined })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("confirmation")));
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
        acceptanceCoverage: [{ acceptanceId: "AC-TEST-01", coverage: "full" }],
        method: "automated",
        result: "pass",
        freshness: "stale",
        capabilities: ["automated"],
        scope: "Node test fixture",
        testRefs: ["fixture::automated"],
        limitations: ["Does not exercise WebView2"],
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
  assert.ok(errors.some((error) => error.includes("缺少证据能力 automated")));
});

test("automated evidence cannot impersonate required target-environment evidence", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({
      validationStatus: "validated",
      validationSlices: [{
        id: "AC-TEST-01",
        components: ["frontend.features"],
        requiredEvidence: ["automated", "windows_webview2"],
      }],
      evidence: [{
        id: "EV-TEST-01",
        acceptanceIds: ["AC-TEST-01"],
        acceptanceCoverage: [{ acceptanceId: "AC-TEST-01", coverage: "full" }],
        method: "automated",
        result: "pass",
        freshness: "current",
        capabilities: ["automated"],
        scope: "Reducer and component tests",
        testRefs: ["fixture::component"],
        limitations: ["No real WebView2 input"],
        sourceRevision: "7026768",
        worktreeState: "clean",
        environment: "Node test fixture",
        validatedAt: "2026-08-26",
      }],
    })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("缺少证据能力 windows_webview2")));
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

test("impact report does not rewrite dossiers and partial features remain non-blocking", () => {
  const before = digestFeatureFiles();
  const result = spawnSync(process.execPath, [
    "scripts/check-architecture-docs.mjs",
    "--impact",
    "--base",
    "7026768",
  ], {
    cwd: root,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Potentially Stale/);
  assert.equal(digestFeatureFiles(), before);
  assert.match(result.stdout, /declaredStatus=partial; effectiveFreshness=potentially_stale/);
});

test("effective freshness detects source changes after otherwise current evidence", () => {
  const entry = dossier({
    evidence: [{
      id: "EV-TEST-01",
      acceptanceIds: ["AC-TEST-01"],
      acceptanceCoverage: [{ acceptanceId: "AC-TEST-01", coverage: "full" }],
      method: "automated",
      result: "pass",
      freshness: "current",
      capabilities: ["automated"],
      scope: "Shortcut frontend automation",
      testRefs: ["fixture::shortcut"],
      limitations: [],
      sourceRevision: "7026768",
      worktreeState: "clean",
      environment: "Node test fixture",
      validatedAt: "2026-08-26",
    }],
  });
  const map = {
    components: [{
      id: "frontend.features",
      source: ["src/features/shortcut"],
      dependsOn: [],
    }],
  };
  assert.equal(effectiveSliceFreshness(entry, entry.metadata.validationSlices[0], map), "potentially_stale");
});

test("user documentation describes Unicode SendInput as the default delivery path", () => {
  const readme = readFileSync(path.join(root, "README.md"), "utf8");
  const troubleshooting = readFileSync(path.join(root, "docs/troubleshooting.md"), "utf8");
  assert.match(readme, /Unicode `SendInput`/);
  assert.doesNotMatch(readme, /committed once through clipboard paste/i);
  assert.match(troubleshooting, /Unicode `SendInput`/);
  assert.match(troubleshooting, /compatibility/i);
});

test("implemented dossier rejects known architecture deviations", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({
      implementationReview: {
        status: "deviating",
        sourceRevision: "7026768",
        worktreeState: "dirty",
        changedPaths: ["src/example.rs"],
        reviewedAt: "2026-08-27",
        summary: "Actor ownership is incomplete",
        knownDeviations: ["detached task mutates runtime"],
      },
    })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("implementationReview/status")));
});

test("implementation review must bind to a resolvable source revision", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({
      implementationReview: {
        status: "conformant",
        sourceRevision: "deadbee",
        worktreeState: "clean",
        reviewedAt: "2026-08-27",
        summary: "Unresolvable revision fixture",
        knownDeviations: [],
      },
    })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("implementationReview.sourceRevision 无法解析")));
});

test("evidence must declare exact per-acceptance coverage", () => {
  const errors = [];
  validateFeatureDossiers(
    [dossier({
      evidence: [{
        id: "EV-TEST-01",
        acceptanceIds: ["AC-TEST-01"],
        acceptanceCoverage: [{ acceptanceId: "AC-OTHER-01", coverage: "full" }],
        method: "automated",
        result: "pass",
        freshness: "current",
        capabilities: ["automated"],
        scope: "Mismatched coverage fixture",
        testRefs: ["fixture::mismatch"],
        limitations: [],
        sourceRevision: "7026768",
        worktreeState: "clean",
        environment: "Node test fixture",
        validatedAt: "2026-08-27",
      }],
    })],
    validator(),
    new Set(["frontend.features"]),
    new Set(["ADR-0010"]),
    errors,
  );
  assert.ok(errors.some((error) => error.includes("acceptanceCoverage 包含未关联验收")));
  assert.ok(errors.some((error) => error.includes("未逐项声明 acceptanceCoverage")));
});

test("Current views require source binding and deviation-aware review metadata", () => {
  const directory = mkdtempSync(path.join(tmpdir(), "gy-current-view-"));
  try {
    const file = path.join(directory, "c4-test.md");
    writeFileSync(file, '---\n{"documentType":"c4-view","viewStatus":"current"}\n---\n', "utf8");
    const missingErrors = [];
    validateCurrentViews(directory, missingErrors);
    assert.ok(missingErrors.some((error) => error.includes("sourceRevision")));
    assert.ok(missingErrors.some((error) => error.includes("knownDeviations")));

    writeFileSync(
      file,
      '---\n{"documentType":"c4-view","viewStatus":"current","sourceRevision":"7026768","worktreeState":"clean","reviewStatus":"reviewed","reviewedAt":"2026-08-27","knownDeviations":[]}\n---\n',
      "utf8",
    );
    const validErrors = [];
    validateCurrentViews(directory, validErrors);
    assert.deepEqual(validErrors, []);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("implemented voice control claim is blocked while SharedRuntime writers remain", () => {
  const check = {
    implementationClaimChecks: [{
      featureId: "FEAT-VOICE-INPUT-CONTROL-PLANE",
      whenImplementationStatus: "implemented",
      forbiddenSourceTokens: [{
        path: "src-tauri/src/voice_controller.rs",
        token: "type SharedRuntime = Arc<Mutex<VoiceRuntime>>",
        reason: "Actor must own the aggregate by value",
      }],
    }],
  };
  const implemented = dossier({
    featureId: "FEAT-VOICE-INPUT-CONTROL-PLANE",
    implementationStatus: "implemented",
  });
  const errors = [];
  validateImplementationClaims(check, [implemented], errors);
  assert.ok(errors.some((error) => error.includes("SharedRuntime")));

  const inProgressErrors = [];
  validateImplementationClaims(
    check,
    [dossier({ featureId: "FEAT-VOICE-INPUT-CONTROL-PLANE", implementationStatus: "in_progress" })],
    inProgressErrors,
  );
  assert.deepEqual(inProgressErrors, []);
});

test("oversized architecture component produces a non-blocking cohesion warning", () => {
  const warnings = collectCohesionWarnings({
    cohesionReviewThresholds: [{
      path: "src-tauri/src/voice_controller.rs",
      maxLines: 800,
      reason: "review controller cohesion",
    }],
  });
  assert.equal(warnings.length, 1);
  assert.match(warnings[0], /超过复核阈值 800/);
});
