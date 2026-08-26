import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const configPath = path.join(root, "docs", "architecture", "architecture.config.json");
const impactMode = process.argv.includes("--impact");
const baseArgumentIndex = process.argv.indexOf("--base");
const baseRef = baseArgumentIndex >= 0 ? process.argv[baseArgumentIndex + 1] : undefined;

function repoPath(value) {
  return value.replaceAll("\\", "/").replace(/^\.\//, "");
}

function insideRoot(absolutePath) {
  const relative = path.relative(root, absolutePath);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function readJson(absolutePath, label) {
  if (!existsSync(absolutePath)) throw new Error(`缺少 ${label}: ${repoPath(path.relative(root, absolutePath))}`);
  return JSON.parse(readFileSync(absolutePath, "utf8"));
}

function parseJsonFrontMatterText(contents, label = "document") {
  const match = contents.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) throw new Error(`${label}: 缺少 JSON front matter`);
  try {
    return { metadata: JSON.parse(match[1]), body: contents.slice(match[0].length) };
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`${label}: JSON front matter 无效：${detail}`);
  }
}

function parseJsonFrontMatter(file) {
  return parseJsonFrontMatterText(readFileSync(file, "utf8"), repoPath(path.relative(root, file)));
}

function loadArchitecture() {
  const config = readJson(configPath, "架构配置");
  const architectureDir = path.resolve(root, config.architectureDir);
  const featureDir = path.resolve(root, config.featureDir);
  const proposalDir = path.resolve(root, config.proposalDir);
  const postmortemDir = path.resolve(root, config.postmortemDir);
  for (const [label, directory] of [
    ["architectureDir", architectureDir],
    ["featureDir", featureDir],
    ["proposalDir", proposalDir],
    ["postmortemDir", postmortemDir],
  ]) {
    if (!insideRoot(directory)) throw new Error(`${label} 不能逃出仓库`);
    if (!existsSync(directory)) throw new Error(`缺少 ${label}: ${repoPath(path.relative(root, directory))}`);
  }
  return {
    config,
    architectureDir,
    featureDir,
    proposalDir,
    postmortemDir,
    map: readJson(path.resolve(root, config.codeMap), "代码地图"),
    schema: readJson(path.resolve(root, config.codeMapSchema), "代码地图 Schema"),
    featureSchema: readJson(path.resolve(root, config.featureSchema), "Feature Dossier Schema"),
    facts: readJson(path.resolve(root, config.facts), "架构事实"),
  };
}

function walk(directory, predicate = () => true) {
  const output = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) output.push(...walk(absolute, predicate));
    if (entry.isFile() && predicate(absolute)) output.push(absolute);
  }
  return output;
}

function gitLines(args) {
  try {
    return execFileSync("git", args, {
      cwd: root,
      encoding: "utf8",
      // Git may emit platform line-ending advice; impact output should stay actionable.
      stdio: ["ignore", "pipe", "ignore"],
    })
      .split(/\r?\n/)
      .map(repoPath)
      .filter(Boolean);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`无法读取 Git 变更（${args.join(" ")}）：${detail}`);
  }
}

function changedFiles(base) {
  const files = [
    ...gitLines(["diff", "--name-only"]),
    ...gitLines(["diff", "--name-only", "--cached"]),
    ...gitLines(["ls-files", "--others", "--exclude-standard"]),
  ];
  if (base) files.push(...gitLines(["diff", "--name-only", `${base}...HEAD`]));
  return new Set(files);
}

function sourceMatches(source, changed) {
  if (changed === source) return true;
  const absolute = path.join(root, source);
  return existsSync(absolute) && statSync(absolute).isDirectory() && changed.startsWith(`${source}/`);
}

function loadFeatureDossiers(featureDir) {
  return readdirSync(featureDir)
    .filter((name) => name.endsWith(".md") && name !== "README.md")
    .sort()
    .map((name) => {
      const file = path.join(featureDir, name);
      return { file, ...parseJsonFrontMatter(file) };
    });
}

function affectedValidationSlices(dossiers, affectedIds) {
  return dossiers.flatMap((dossier) =>
    (dossier.metadata.validationSlices ?? [])
      .filter((slice) => slice.components.some((id) => affectedIds.has(id)))
      .map((slice) => ({ featureId: dossier.metadata.featureId, sliceId: slice.id })),
  );
}

function runImpact(architecture) {
  if (baseArgumentIndex >= 0 && !baseRef) throw new Error("--base 需要 Git ref，例如 origin/main");
  const { map, featureDir } = architecture;
  const dossiers = loadFeatureDossiers(featureDir);
  const changed = changedFiles(baseRef);
  const directSource = new Set();
  const directDocs = new Set();

  for (const component of map.components) {
    if ([...changed].some((file) => component.source.some((entry) => sourceMatches(repoPath(entry), file)))) {
      directSource.add(component.id);
    }
    const narrative = [...component.docs, ...component.adrs].map(repoPath);
    if ([...changed].some((file) => narrative.includes(file))) directDocs.add(component.id);
  }

  for (const dossier of dossiers) {
    const relative = repoPath(path.relative(root, dossier.file));
    if (!changed.has(relative)) continue;
    for (const componentId of dossier.metadata.components ?? []) {
      directDocs.add(componentId);
    }
  }

  const reasons = new Map();
  for (const id of directSource) reasons.set(id, new Set(["源码直接变更"]));
  for (const id of directDocs) {
    if (!reasons.has(id)) reasons.set(id, new Set());
    reasons.get(id).add("文档或 ADR 直接变更");
  }

  let expanded = true;
  while (expanded) {
    expanded = false;
    for (const component of map.components) {
      if (reasons.has(component.id)) continue;
      const changedDependency = component.dependsOn.find((id) => reasons.has(id));
      if (changedDependency) {
        reasons.set(component.id, new Set([`依赖组件 ${changedDependency} 发生变化`]));
        expanded = true;
      }
    }
  }

  const affected = map.components.filter((component) => reasons.has(component.id));
  const baseSummary = baseRef ? `，包含 ${baseRef}...HEAD` : "";
  console.log(`架构影响分析：检测到 ${changed.size} 个文件${baseSummary}。`);
  if (affected.length === 0) {
    console.log("没有匹配到代码地图组件。若变更新增了架构边界，请先在 code-map.json 注册组件。");
    return;
  }

  for (const component of affected) {
    console.log(`\n- ${component.id} — ${component.name} [${component.status}]`);
    console.log(`  Owner：${component.owner}`);
    console.log(`  命中原因：${[...reasons.get(component.id)].join("；")}`);
    console.log(`  触发条件：${component.changeTriggers.join("；")}`);
    console.log(`  复核文档：${component.docs.join(", ")}`);
    if (component.adrs.length > 0) console.log(`  相关 ADR：${component.adrs.join(", ")}`);
  }

  const affectedIds = new Set(affected.map((component) => component.id));
  const slices = affectedValidationSlices(dossiers, affectedIds);
  if (slices.length > 0) {
    console.log("\nPotentially Stale 验收切片（仅提示，不自动修改证据状态）：");
    for (const slice of slices) {
      console.log(`- ${slice.featureId} / ${slice.sliceId}`);
    }
  }
}

function validateRelativeLinks(markdownFiles, errors) {
  const linkPattern = /!?\[[^\]]*\]\(([^)]+)\)/g;
  for (const file of markdownFiles) {
    const contents = readFileSync(file, "utf8");
    for (const match of contents.matchAll(linkPattern)) {
      let target = match[1].trim();
      if (target.startsWith("<") && target.endsWith(">")) target = target.slice(1, -1);
      target = target.split(/\s+"/, 1)[0];
      if (/^(https?:|mailto:|#)/i.test(target)) continue;
      const withoutAnchor = target.split("#", 1)[0].split("?", 1)[0];
      if (!withoutAnchor) continue;
      let decoded;
      try {
        decoded = decodeURIComponent(withoutAnchor);
      } catch {
        errors.push(`${repoPath(path.relative(root, file))}: 无法解码链接 ${target}`);
        continue;
      }
      const resolved = path.resolve(path.dirname(file), decoded);
      if (!insideRoot(resolved)) {
        errors.push(`${repoPath(path.relative(root, file))}: 链接逃出仓库 ${target}`);
      } else if (!existsSync(resolved)) {
        errors.push(`${repoPath(path.relative(root, file))}: 失效链接 ${target}`);
      }
    }
  }
}

function validateFences(markdownFiles, errors) {
  for (const file of markdownFiles) {
    const contents = readFileSync(file, "utf8");
    const fences = contents.match(/^```/gm)?.length ?? 0;
    if (fences % 2 !== 0) errors.push(`${repoPath(path.relative(root, file))}: Markdown 代码围栏未闭合`);
  }
}

async function validateMermaid(markdownFiles, errors) {
  // Mermaid's flowchart parser initializes DOMPurify from a browser-like global.
  // happy-dom is already used by the frontend tests and avoids a Chromium dependency.
  const { Window } = await import("happy-dom");
  const browserWindow = new Window();
  globalThis.window = browserWindow;
  globalThis.document = browserWindow.document;
  Object.defineProperty(globalThis, "navigator", { value: browserWindow.navigator, configurable: true });
  const { default: mermaid } = await import("mermaid");
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
  let count = 0;
  for (const file of markdownFiles) {
    const contents = readFileSync(file, "utf8");
    for (const match of contents.matchAll(/```mermaid\s*\r?\n([\s\S]*?)```/g)) {
      count += 1;
      try {
        await mermaid.parse(match[1]);
      } catch (error) {
        const message = error instanceof Error ? error.message.split(/\r?\n/, 1)[0] : String(error);
        errors.push(`${repoPath(path.relative(root, file))}: Mermaid 图 ${count} 语法错误：${message}`);
      }
    }
  }
  return count;
}

function validateAdrs(architectureDir, errors) {
  const adrDir = path.join(architectureDir, "adr");
  const index = readFileSync(path.join(adrDir, "README.md"), "utf8");
  const files = readdirSync(adrDir)
    .filter((name) => /^\d{4}-[a-z0-9-]+\.md$/.test(name))
    .sort();

  for (const name of files) {
    const contents = readFileSync(path.join(adrDir, name), "utf8");
    const number = name.slice(0, 4);
    if (!contents.startsWith(`# ADR-${number}`)) {
      errors.push(`docs/architecture/adr/${name}: 标题编号与文件名不一致`);
    }
    if (!/^- Status: (Proposed|Accepted|Rejected|Deprecated|Superseded)$/m.test(contents)) {
      errors.push(`docs/architecture/adr/${name}: 缺少合法 Status`);
    }
    if (!/^- Date: \d{4}-\d{2}-\d{2}$/m.test(contents)) {
      errors.push(`docs/architecture/adr/${name}: 缺少 ISO Date`);
    }
    if (!/^- Deciders: .+$/m.test(contents)) {
      errors.push(`docs/architecture/adr/${name}: 缺少 Deciders`);
    }
    if (Number(number) >= 9) {
      for (const field of ["Drivers", "Related features", "Assumptions", "Evidence"]) {
        if (!new RegExp(`^- ${field}: .+`, "m").test(contents)) {
          errors.push(`docs/architecture/adr/${name}: 缺少 ${field}`);
        }
      }
    }
    if (!index.includes(`(${name})`)) {
      errors.push(`docs/architecture/adr/${name}: 未登记在 ADR 索引`);
    }
  }
  return files.length;
}

function validateDependencyGraph(map, errors) {
  const ids = new Set(map.components.map((component) => component.id));
  for (const component of map.components) {
    for (const dependency of component.dependsOn) {
      if (!ids.has(dependency)) errors.push(`${component.id}: dependsOn 引用了未知组件 ${dependency}`);
      if (dependency === component.id) errors.push(`${component.id}: 不能依赖自身`);
    }
  }

  const visiting = new Set();
  const visited = new Set();
  function visit(id, trail) {
    if (visiting.has(id)) {
      errors.push(`组件依赖形成环：${[...trail, id].join(" -> ")}`);
      return;
    }
    if (visited.has(id)) return;
    visiting.add(id);
    const component = map.components.find((entry) => entry.id === id);
    for (const dependency of component?.dependsOn ?? []) visit(dependency, [...trail, id]);
    visiting.delete(id);
    visited.add(id);
  }
  for (const id of ids) visit(id, []);
}

function validateSourceCoverage(config, map, errors) {
  const mappedSources = [...new Set(map.components.flatMap((component) => component.source).map(repoPath))];
  const productionFiles = [];

  for (const rule of config.sourceCoverage) {
    const coverageRoot = path.resolve(root, rule.root);
    if (!insideRoot(coverageRoot) || !existsSync(coverageRoot)) {
      errors.push(`sourceCoverage root 无效：${rule.root}`);
      continue;
    }
    productionFiles.push(
      ...walk(coverageRoot, (absolute) => {
        const name = path.basename(absolute);
        return rule.extensions.includes(path.extname(name)) &&
          !rule.excludeSuffixes.some((suffix) => name.endsWith(suffix));
      }).map((absolute) => repoPath(path.relative(root, absolute))),
    );
  }

  const unmapped = productionFiles.filter(
    (file) => !mappedSources.some((source) => sourceMatches(source, file)),
  );
  for (const file of unmapped) errors.push(`生产源码未映射到组件：${file}`);
  return { mapped: productionFiles.length - unmapped.length, total: productionFiles.length };
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function evaluateArithmetic(expression) {
  const normalized = expression.replaceAll("_", "").trim();
  if (!/^[0-9+\-*/%()\s]+$/.test(normalized)) throw new Error(`不支持的常量表达式：${expression}`);
  // The character allow-list above prevents identifiers, property access, strings and calls.
  const value = Function(`"use strict"; return (${normalized});`)();
  if (!Number.isFinite(value)) throw new Error(`常量表达式不是有限数值：${expression}`);
  return value;
}

function readRustConstant(fact) {
  const absolute = path.resolve(root, fact.source.path);
  const contents = readFileSync(absolute, "utf8");
  const pattern = new RegExp(`(?:pub\\s+)?const\\s+${escapeRegex(fact.source.symbol)}\\s*:[^=]+?=\\s*([^;]+);`);
  const match = contents.match(pattern);
  if (!match) throw new Error(`找不到常量 ${fact.source.symbol}`);
  let expression = match[1].trim();
  if (fact.source.wrapper) {
    const wrapper = new RegExp(`^${escapeRegex(fact.source.wrapper)}\\((.*)\\)$`);
    const wrapped = expression.match(wrapper);
    if (!wrapped) throw new Error(`常量 ${fact.source.symbol} 不符合 wrapper ${fact.source.wrapper}`);
    expression = wrapped[1];
  }
  return evaluateArithmetic(expression);
}

function validateFacts(factsDocument, markdownFiles, errors) {
  const ids = new Set();
  const markdown = markdownFiles.map((file) => readFileSync(file, "utf8")).join("\n");
  const markerIds = new Set(
    [...markdown.matchAll(/\[fact:([a-z0-9.-]+)\]/g)].map((match) => match[1]),
  );

  for (const fact of factsDocument.facts ?? []) {
    if (ids.has(fact.id)) errors.push(`architecture-facts.json: 重复 fact ID ${fact.id}`);
    ids.add(fact.id);
    try {
      const actual = readRustConstant(fact);
      if (actual !== fact.value) {
        errors.push(`${fact.id}: 代码值为 ${actual}，事实清单值为 ${fact.value}`);
      }
    } catch (error) {
      errors.push(`${fact.id}: ${error instanceof Error ? error.message : String(error)}`);
    }

    if (!markerIds.has(fact.id)) errors.push(`${fact.id}: 缺少 Markdown fact marker`);
    for (const mention of fact.mentions ?? []) {
      const absolute = path.resolve(root, mention.path);
      if (!insideRoot(absolute) || !existsSync(absolute)) {
        errors.push(`${fact.id}: mention 路径不存在 ${mention.path}`);
        continue;
      }
      const token = mention.token
        .replaceAll("{value}", String(fact.value))
        .replaceAll("{display}", fact.display);
      if (!readFileSync(absolute, "utf8").includes(token)) {
        errors.push(`${fact.id}: ${mention.path} 缺少当前值文本“${token}”`);
      }
    }
  }
  for (const id of markerIds) {
    if (!ids.has(id)) errors.push(`Markdown 使用了未登记 fact marker ${id}`);
  }
  return ids.size;
}

function validateFeatureDossiers(dossiers, validateFeature, componentIds, adrIds, errors) {
  const featureIds = new Set();
  const requiredHeadings = [
    "用户目标",
    "验收场景",
    "明确不规定的实现",
    "局部假设",
    "架构决策",
    "当前实现入口",
    "验证状态",
    "澄清历史",
  ];

  for (const dossier of dossiers) {
    const label = repoPath(path.relative(root, dossier.file));
    if (!validateFeature(dossier.metadata)) {
      for (const error of validateFeature.errors ?? []) {
        errors.push(`${label}: metadata${error.instancePath || "/"} ${error.message}`);
      }
      continue;
    }

    const metadata = dossier.metadata;
    if (featureIds.has(metadata.featureId)) errors.push(`${label}: 重复 Feature ID ${metadata.featureId}`);
    featureIds.add(metadata.featureId);

    const sliceIds = new Set();
    for (const componentId of metadata.components) {
      if (!componentIds.has(componentId)) errors.push(`${label}: 引用未知组件 ${componentId}`);
    }
    for (const decision of metadata.decisions) {
      if (!adrIds.has(decision)) errors.push(`${label}: 引用未知 ADR ${decision}`);
    }
    for (const slice of metadata.validationSlices) {
      if (sliceIds.has(slice.id)) errors.push(`${label}: 重复验收 ID ${slice.id}`);
      sliceIds.add(slice.id);
      if (!dossier.body.includes(slice.id)) errors.push(`${label}: 正文未登记验收 ${slice.id}`);
      for (const componentId of slice.components) {
        if (!componentIds.has(componentId)) errors.push(`${label}: ${slice.id} 引用未知组件 ${componentId}`);
      }
    }

    const evidenceIds = new Set();
    for (const evidence of metadata.evidence) {
      if (evidenceIds.has(evidence.id)) errors.push(`${label}: 重复证据 ID ${evidence.id}`);
      evidenceIds.add(evidence.id);
      for (const acceptanceId of evidence.acceptanceIds) {
        if (!sliceIds.has(acceptanceId)) errors.push(`${label}: ${evidence.id} 引用未知验收 ${acceptanceId}`);
      }
    }

    if (metadata.validationStatus === "validated") {
      for (const slice of metadata.validationSlices) {
        const currentPass = metadata.evidence.some(
          (evidence) =>
            evidence.acceptanceIds.includes(slice.id) &&
            evidence.result === "pass" &&
            ["current", "revalidated"].includes(evidence.freshness),
        );
        if (!currentPass) errors.push(`${label}: validated 但 ${slice.id} 缺少当前成功证据`);
      }
    }

    for (const heading of requiredHeadings) {
      if (!dossier.body.includes(`## ${heading}`)) errors.push(`${label}: 缺少章节“${heading}”`);
    }
  }
  return featureIds;
}

function validateCurrentViews(architectureDir, errors) {
  const files = readdirSync(architectureDir)
    .filter((name) => /^c4-.*\.md$/.test(name) || name === "runtime-views.md")
    .map((name) => path.join(architectureDir, name));
  for (const file of files) {
    const label = repoPath(path.relative(root, file));
    const { metadata } = parseJsonFrontMatter(file);
    if (metadata.viewStatus !== "current") errors.push(`${label}: Current 视图必须声明 viewStatus=current`);
    if (!["c4-view", "runtime-view"].includes(metadata.documentType)) {
      errors.push(`${label}: 缺少合法 documentType`);
    }
  }
}

function validateProposals(proposalDir, featureIds, errors) {
  const files = walk(proposalDir, (file) => file.endsWith(".md") && path.basename(file) !== "README.md");
  for (const file of files) {
    const label = repoPath(path.relative(root, file));
    const { metadata } = parseJsonFrontMatter(file);
    if (metadata.documentType !== "architecture-proposal" || metadata.viewStatus !== "proposed") {
      errors.push(`${label}: Proposal 必须声明 documentType=architecture-proposal 且 viewStatus=proposed`);
    }
    for (const field of ["owner", "createdAt", "revisitWhen"]) {
      if (typeof metadata[field] !== "string" || metadata[field].trim() === "") {
        errors.push(`${label}: 缺少 ${field}`);
      }
    }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(metadata.createdAt ?? "")) {
      errors.push(`${label}: createdAt 必须是 ISO 日期`);
    }
    if (!Array.isArray(metadata.relatedFeatures)) {
      errors.push(`${label}: relatedFeatures 必须是数组`);
    } else {
      for (const featureId of metadata.relatedFeatures) {
        if (!featureIds.has(featureId)) errors.push(`${label}: 引用未知 Feature ${featureId}`);
      }
    }
  }
}

function validatePostmortems(postmortemDir, errors) {
  const files = walk(postmortemDir, (file) => file.endsWith(".md"));
  for (const file of files) {
    const label = repoPath(path.relative(root, file));
    const { metadata } = parseJsonFrontMatter(file);
    if (metadata.documentType !== "postmortem" || metadata.normative !== false) {
      errors.push(`${label}: Postmortem 必须声明 documentType=postmortem 且 normative=false`);
    }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(metadata.incidentDate ?? "")) {
      errors.push(`${label}: incidentDate 必须是 ISO 日期`);
    }
    if (!Array.isArray(metadata.affectedRevisions) || metadata.affectedRevisions.length === 0) {
      errors.push(`${label}: affectedRevisions 不能为空`);
    }
  }
}

function validateProposalReferences(map, proposalPrefix, errors) {
  for (const component of map.components ?? []) {
    for (const documentPath of [...component.docs, ...component.adrs].map(repoPath)) {
      if (documentPath.startsWith(proposalPrefix)) {
        errors.push(`code-map.json: ${component.id} 不得引用 Proposed 文档 ${documentPath}`);
      }
    }
  }
}

async function runCheck(architecture) {
  const {
    config,
    architectureDir,
    featureDir,
    proposalDir,
    postmortemDir,
    map,
    schema,
    featureSchema,
    facts,
  } = architecture;
  const errors = [];

  const ajv = new Ajv2020({
    allErrors: true,
    strict: true,
    formats: { date: /^\d{4}-\d{2}-\d{2}$/ },
  });
  const validateMap = ajv.compile(schema);
  const validateFeature = ajv.compile(featureSchema);
  if (!validateMap(map)) {
    for (const error of validateMap.errors ?? []) {
      errors.push(`code-map.schema: ${error.instancePath || "/"} ${error.message}`);
    }
  }

  for (const relative of config.requiredDocs) {
    if (!existsSync(path.join(architectureDir, relative))) {
      errors.push(`缺少必需文档 ${repoPath(path.relative(root, path.join(architectureDir, relative)))}`);
    }
  }

  const ids = new Set();
  for (const component of map.components ?? []) {
    if (ids.has(component.id)) errors.push(`code-map.json: 重复组件 ID ${component.id}`);
    ids.add(component.id);
    for (const field of ["source", "docs", "adrs"]) {
      for (const relativeValue of component[field] ?? []) {
        const relative = repoPath(relativeValue);
        const absolute = path.resolve(root, relative);
        if (!insideRoot(absolute)) {
          errors.push(`code-map.json: ${component.id} 路径逃出仓库 ${relative}`);
        } else if (!existsSync(absolute)) {
          errors.push(`code-map.json: ${component.id} 路径不存在 ${relative}`);
        }
      }
    }
  }

  const proposalPrefix = `${repoPath(path.relative(root, proposalDir))}/`;
  validateProposalReferences(map, proposalPrefix, errors);

  validateDependencyGraph(map, errors);
  const coverage = validateSourceCoverage(config, map, errors);
  const architectureMarkdownFiles = walk(architectureDir, (absolute) => absolute.endsWith(".md"));
  const currentMarkdownFiles = architectureMarkdownFiles.filter(
    (file) => !repoPath(path.relative(root, file)).startsWith(proposalPrefix),
  );
  const dossierMarkdownFiles = walk(featureDir, (absolute) => absolute.endsWith(".md"));
  const postmortemMarkdownFiles = walk(postmortemDir, (absolute) => absolute.endsWith(".md"));
  const entryMarkdownFiles = (config.knowledgeEntries ?? []).map((entry) => path.resolve(root, entry));
  const markdownFiles = [...new Set([
    ...architectureMarkdownFiles,
    ...dossierMarkdownFiles,
    ...postmortemMarkdownFiles,
    ...entryMarkdownFiles,
  ])];
  const markdown = currentMarkdownFiles.map((file) => readFileSync(file, "utf8")).join("\n");
  const documentedIds = new Set(
    [...markdown.matchAll(/\[component:([a-z0-9.-]+)\]/g)].map((match) => match[1]),
  );
  for (const id of ids) {
    if (!documentedIds.has(id)) errors.push(`组件 ${id} 没有 Markdown marker`);
  }
  for (const id of documentedIds) {
    if (!ids.has(id)) errors.push(`Markdown 使用了未注册组件 marker ${id}`);
  }


  const adrIds = new Set(
    readdirSync(path.join(architectureDir, "adr"))
      .filter((name) => /^\d{4}-[a-z0-9-]+\.md$/.test(name))
      .map((name) => `ADR-${name.slice(0, 4)}`),
  );
  const dossiers = loadFeatureDossiers(featureDir);
  const featureIds = validateFeatureDossiers(dossiers, validateFeature, ids, adrIds, errors);
  validateCurrentViews(architectureDir, errors);
  validateProposals(proposalDir, featureIds, errors);
  validatePostmortems(postmortemDir, errors);
  validateRelativeLinks(markdownFiles, errors);
  validateFences(markdownFiles, errors);
  const mermaidCount = await validateMermaid(markdownFiles, errors);
  const adrCount = validateAdrs(architectureDir, errors);
  const factCount = validateFacts(facts, currentMarkdownFiles, errors);

  if (errors.length > 0) {
    console.error("文档结构与追踪关系检查失败：");
    for (const error of errors) console.error(`- ${error}`);
    process.exitCode = 1;
    return;
  }

  console.log(
    `文档结构与追踪关系检查通过：${map.components.length} 个组件，${coverage.mapped}/${coverage.total} 个生产源码文件，${dossiers.length} 份 Feature Dossier，${markdownFiles.length} 个 Markdown 文件，${mermaidCount} 张 Mermaid 图，${factCount} 条代码不变量，${adrCount} 条 ADR；该结果不证明架构语义或目标环境验收通过。`,
  );
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const architecture = loadArchitecture();
    if (impactMode) runImpact(architecture);
    else await runCheck(architecture);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

export { affectedValidationSlices, parseJsonFrontMatterText, validateFeatureDossiers, validatePostmortems, validateProposalReferences, validateProposals };
