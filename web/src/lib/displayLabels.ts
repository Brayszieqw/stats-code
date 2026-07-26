/**
 * displayLabels — 契约标识符 → 用户可读中文标签的唯一映射源。
 *
 * 后端契约用 snake_case 标识符（`model_linear`、`survival_km`、
 * `PRIMARY_KEY_UNBOUND`）和英文枚举（`Numeric`、`Categorical`）。这些是数据
 * 契约，不能改；但直接显示给用户会漏出实现细节——界面上出现「model_linear」
 * 或「文本型 (String)」这种半英文标签，对临床科研用户是噪音。
 *
 * 所有展示层都从这里取标签。未收录的标识符走 humanizeIdentifier 兜底，
 * 至少把下划线换成空格并首字母大写，不会把裸 snake_case 抛给用户。
 */

/** 列的推断类型。契约枚举见 ts-backend contract/domain.ts 的 columnType。 */
export const COLUMN_TYPE_LABELS: Record<string, string> = {
  Numeric: '数值',
  Categorical: '分类',
  Date: '日期',
  String: '文本',
};

/**
 * 统计方法短标签，用于标签胶囊与表头（审批弹窗用 analysisPreflight 的
 * METHOD_LABELS 完整方法名）。
 *
 * 必须同时收录两套命名，因为界面上两者都会出现：
 *  - skill_id：前端发起请求时用的 id（`model_linear`、`survival_km`）；
 *  - algorithm_id：服务端回传在 analysis 元数据里的 Output-Level 算法 id
 *    （`linear`、`kaplan_meier`），由 skill-algorithm-map.ts 映射得出。
 *
 * 两者并不相等。只收 skill_id 会让 ProModeView 的方法胶囊拿不到映射，
 * 走 humanize 兜底显示成 `Linear`，再被 CSS text-transform:uppercase 变成
 * 「LINEAR」——这正是真机上看到的英文标签。
 */
export const METHOD_SHORT_LABELS: Record<string, string> = {
  // skill_id
  tableone: '基线特征表',
  ttest: 'T 检验',
  anova: '方差分析',
  correlation: '相关分析',
  survival_km: 'KM 生存分析',
  model_linear: '线性回归',
  model_logistic: 'Logistic 回归',
  model_cox: 'Cox 回归',
  power: '功效分析',
  inspect: '数据检视',
  // algorithm_id（tableone/ttest/anova/correlation 两套同名，不再重复）
  linear: '线性回归',
  logistic: 'Logistic 回归',
  cox: 'Cox 回归',
  kaplan_meier: 'KM 生存分析',
};

/**
 * 把未收录的标识符转成人眼可读形式：下划线→空格，首字母大写。
 * 这是兜底，不是主路径——新增技能应当同时在上面的表里登记。
 *
 * 入参故意放宽到 unknown：契约上 algorithm_id 是必填 string，但实测历史
 * 会话里存在缺该字段的 analysis 记录（真机 ProModeView 因此白屏崩溃）。
 * 标签函数属于纯展示层，为一个缺失字段整页崩掉是不可接受的取舍，
 * 因此这里吞掉非字符串输入并返回占位符。
 */
export function humanizeIdentifier(value: unknown): string {
  if (typeof value !== 'string' || value.trim().length === 0) return '未知';
  const spaced = value.replace(/_/g, ' ').trim();
  if (spaced.length === 0) return value;
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/** 列类型标签；未知类型兜底为 humanize 后的原值。 */
export function columnTypeLabel(type: unknown): string {
  if (typeof type === 'string' && type in COLUMN_TYPE_LABELS) return COLUMN_TYPE_LABELS[type]!;
  return humanizeIdentifier(type);
}

/** 方法短标签；未知方法兜底为 humanize 后的原值。 */
export function methodShortLabel(id: unknown): string {
  if (typeof id === 'string' && id in METHOD_SHORT_LABELS) return METHOD_SHORT_LABELS[id]!;
  return humanizeIdentifier(id);
}
