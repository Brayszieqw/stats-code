import { Alert, List, Tag, Typography } from 'antd';
import { SafetyCertificateOutlined } from '@ant-design/icons';
import { ANALYSIS_TRUST_STATEMENT } from '../lib/analysisPreflight';

const { Paragraph, Text, Title } = Typography;

/**
 * 「当前支持」只列**有分析入口、能真跑出结果**的方法——与
 * ts-backend skill-registry.ts 注册的 skillId 一一对应（tableone / ttest /
 * anova / correlation / model_linear / model_logistic / model_cox /
 * survival_km / power / inspect）。
 *
 * 引擎里另有 nonparametric、rank、epi、diagnostic、standardization 等模块，
 * 但尚未注册成技能，用户无法从任何入口调用。把它们写进「当前支持」是过度
 * 承诺：用户照着找不到入口，或误以为某个结果用了这些方法。因此改列进下方
 * 的「引擎已实现但暂无入口」一节，如实区分「能用」与「有代码」。
 */
const SUPPORTED = [
  '描述统计与基线特征表（Table One，含组间检验）',
  'Welch 双独立样本 T 检验与单因素方差分析',
  'Pearson / Spearman 相关分析',
  '线性、Logistic 与 Cox 回归（分类预测变量自动哑变量编码）',
  'Kaplan–Meier 生存分析',
  '功效与样本量分析',
  '可复现 R / Python / SAS / SPSS 等价代码导出',
];

/** 引擎已实现、但还没有分析入口的方法族——如实告知，避免用户白找。 */
const ENGINE_ONLY = [
  '非参数检验与秩方法',
  '流行病学效应量与诊断试验指标',
  '率的标准化',
];

export function CapabilityStatement() {
  return (
    <article className="capability-statement" aria-label="能力边界">
      <Title level={4}>关于 Stats Code</Title>
      <Paragraph>
        Stats Code 是面向医院科研、公卫与医学院校的本地可审计观察性研究智能体。
        统计值由本机 TypeScript 确定性引擎计算；LLM 只参与意图引导和文字解释，
        不生成或改写数值结果。
      </Paragraph>
      <div className="capability-statement__trust">
        <SafetyCertificateOutlined aria-hidden />
        <strong>{ANALYSIS_TRUST_STATEMENT}</strong>
      </div>

      <Alert
        type="info"
        showIcon
        message="仅提供科研设计与统计分析辅助"
        description="不独立作出诊断、治疗或个体决策；所有研究结论须由具备资质的研究者审核确认。"
        style={{ marginBottom: 16 }}
      />

      <Text strong>当前支持（有分析入口，可直接运行）</Text>
      <List
        size="small"
        dataSource={SUPPORTED}
        renderItem={(item) => <List.Item><Tag color="green">已支持</Tag>{item}</List.Item>}
      />

      <Text strong>引擎已实现，但暂无分析入口</Text>
      <List
        size="small"
        dataSource={ENGINE_ONLY}
        renderItem={(item) => <List.Item><Tag color="orange">暂无入口</Tag>{item}</List.Item>}
      />
      <Paragraph type="secondary" style={{ fontSize: 12, marginTop: -4 }}>
        这些方法的计算代码已在本机引擎中，但尚未提供配置入口，当前版本无法运行；
        不要据此认为某个已出结果用到了它们。
      </Paragraph>

      <Alert
        type="warning"
        showIcon
        message="当前不支持：PSM、TMLE、竞争风险、时空模型与 CDISC。"
        description="这些方法不会被伪装成相近算法执行；超出能力边界时应使用经验证的专业统计软件和人工复核。"
      />
    </article>
  );
}

export default CapabilityStatement;
