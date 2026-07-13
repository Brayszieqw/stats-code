import { Alert, List, Tag, Typography } from 'antd';
import { SafetyCertificateOutlined } from '@ant-design/icons';
import { ANALYSIS_TRUST_STATEMENT } from '../lib/analysisPreflight';

const { Paragraph, Text, Title } = Typography;

const SUPPORTED = [
  '描述统计与 Table One',
  'Welch 双独立样本 T 检验、单因素方差分析与非参数检验',
  '相关分析、流行病学效应量与诊断试验指标',
  '线性、Logistic 与 Cox 回归',
  'Kaplan–Meier 生存分析、功效与样本量分析',
  '标准化、秩检验及可复现 R / Python / SAS / SPSS 等价代码',
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

      <Text strong>当前支持</Text>
      <List
        size="small"
        dataSource={SUPPORTED}
        renderItem={(item) => <List.Item><Tag color="green">已支持</Tag>{item}</List.Item>}
      />

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
