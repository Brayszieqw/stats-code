/**
 * primaryKeyHint — 上传后立刻判断数据集有没有可识别的主键。
 *
 * 服务端审计（ts-backend dataset-audit.ts 的 resolveRoles）只在**审批阶段**
 * 才跑：识别不到主键就抛 PRIMARY_KEY_UNBOUND 阻断执行。问题是用户此时已经
 * 配完分组变量、连续变量、分类变量，才被告知「数据集根本不能用」——白配一遍。
 *
 * 这里把同一套列名判据前移到上传完成的瞬间，只做提示，不碰门禁：真正的
 * 阻断权仍然只在服务端。因此本模块必须与服务端判据保持字面一致，否则会出现
 * 「前端说有主键、审批却拦下」的更糟体验。下面的候选名单与正则是从
 * dataset-audit.ts 的 resolveRoles 逐字搬来的，改一侧必须同步改另一侧。
 */

/** 与服务端 resolveRoles 的 primaryKey 候选列表逐字一致。 */
const PRIMARY_KEY_CANDIDATES = [
  'participant_id',
  'subject_id',
  'patient_id',
  'person_id',
  'record_id',
  'study_id',
  'case_id',
] as const;

/** 与服务端 resolveRoles 的 primaryKey 模式逐字一致。 */
const PRIMARY_KEY_PATTERN = /^(participant|subject|patient|person|record|study|case)_(id|key)$/;

/**
 * 与服务端 conversation/sensitive-data.ts 的 normalizeFieldName 逐字一致：
 * 去首尾空白 → 小写 → 把非「字母/数字/汉字」的连续片段折成单个下划线。
 *
 * 最后那步不是可选的：它让 `Patient ID`、`patient-id`、`patient.id` 都归一到
 * `patient_id` 从而被识别。少了它，前端会对这些表头误报「未识别到主键」，
 * 而服务端审批时其实认得——比不提示更糟。
 */
function normalizeFieldName(value: string): string {
  return value.trim().toLocaleLowerCase().replace(/[^a-z0-9一-鿿]+/g, '_');
}

export interface PrimaryKeyHint {
  /** 服务端能否据列名识别出主键。false 时审批阶段会被 PRIMARY_KEY_UNBOUND 阻断。 */
  resolved: boolean;
  /** 识别到的列名；未识别时为 null。 */
  column: string | null;
}

/**
 * 按服务端判据探测主键列。
 *
 * 只看列名，不看数据——服务端在这一步也只看列名（值的唯一性/空值是之后
 * PRIMARY_KEY_MISSING / DUPLICATE_PRIMARY_KEY 两条独立检查的事）。
 */
export function detectPrimaryKey(columnNames: readonly string[]): PrimaryKeyHint {
  const normalized = new Map(columnNames.map((name) => [normalizeFieldName(name), name]));

  for (const candidate of PRIMARY_KEY_CANDIDATES) {
    const found = normalized.get(candidate);
    if (found) return { resolved: true, column: found };
  }

  const patternMatch = columnNames.find((name) => PRIMARY_KEY_PATTERN.test(normalizeFieldName(name)));
  if (patternMatch) return { resolved: true, column: patternMatch };

  return { resolved: false, column: null };
}

/** 提示文案里列出的可接受列名，供用户改表头时照抄。 */
export const PRIMARY_KEY_NAME_EXAMPLES = PRIMARY_KEY_CANDIDATES.join('、');
