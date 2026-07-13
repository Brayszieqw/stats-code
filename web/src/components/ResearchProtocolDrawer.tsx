import { useEffect } from 'react';
import { Alert, Button, Drawer, Form, Input, Select, Space, Typography } from 'antd';
import type { ResearchProtocol, ResearchProtocolInput } from '../api/types';

const { Paragraph, Text } = Typography;

const EMPTY_PROTOCOL: ResearchProtocolInput = {
  status: 'Draft',
  research_question: '',
  study_design: 'cross_sectional',
  population: '',
  eligibility_criteria: '',
  exposure: '',
  comparator: '',
  outcome: '',
  time_zero: '',
  follow_up: '',
  analysis_unit: '',
  estimand: '',
  confounders: '',
  missing_data_strategy: '',
  primary_analysis: '',
  sensitivity_analysis: '',
};

export const DEMO_PROTOCOL_TEMPLATE: ResearchProtocolInput = {
  status: 'Draft',
  research_question: '在演示成人观察性队列中，吸烟与疾病结局是否相关？',
  study_design: 'cross_sectional',
  population: '纳入演示 CSV 中的成人参与者',
  eligibility_criteria: '纳入有基线记录的参与者；排除重复记录与无法识别的结局',
  exposure: 'smoke（吸烟状态）',
  comparator: '未吸烟参与者',
  outcome: 'disease（二分类疾病结局）',
  time_zero: '基线调查时点',
  follow_up: '横断面分析，不涉及随访',
  analysis_unit: '参与者',
  estimand: '吸烟与疾病患病几率的调整后 OR',
  confounders: 'age、sex、bmi',
  missing_data_strategy: '先报告各变量缺失率；主分析采用完整案例并披露有效样本量',
  primary_analysis: 'Table One 描述基线；多变量 Logistic 回归估计调整后 OR 与 95% CI',
  sensitivity_analysis: '改变协变量集并比较效应估计与置信区间的稳定性',
};

const REQUIRED_FIELDS: Array<keyof ResearchProtocolInput> = [
  'research_question',
  'population',
  'outcome',
  'time_zero',
  'analysis_unit',
  'estimand',
  'primary_analysis',
];

function editableProtocol(protocol: ResearchProtocol | null): ResearchProtocolInput {
  if (!protocol) return EMPTY_PROTOCOL;
  const {
    approved_at: _approvedAt,
    updated_at: _updatedAt,
    version: _version,
    content_sha256: _contentSha256,
    state_sha256: _stateSha256,
    approval_id: _approvalId,
    expected_version: _expectedVersion,
    ...fields
  } = protocol;
  return fields;
}

export interface ResearchProtocolDrawerProps {
  open: boolean;
  protocol: ResearchProtocol | null;
  saving?: boolean;
  readOnly?: boolean;
  error?: string | null;
  onClose: () => void;
  onSave: (input: ResearchProtocolInput) => void | Promise<void>;
}

export function ResearchProtocolDrawer({
  open,
  protocol,
  saving = false,
  readOnly = false,
  error,
  onClose,
  onSave,
}: ResearchProtocolDrawerProps) {
  const [form] = Form.useForm<ResearchProtocolInput>();

  useEffect(() => {
    if (open) form.setFieldsValue(editableProtocol(protocol));
  }, [form, open, protocol]);

  const save = async (status: ResearchProtocolInput['status']) => {
    if (status === 'Approved') await form.validateFields(REQUIRED_FIELDS);
    const values = { ...EMPTY_PROTOCOL, ...form.getFieldsValue(true), status };
    await onSave(values);
  };

  const requiredRule = [{ required: true, whitespace: true, message: '协议审批前必须填写' }];

  return (
    <Drawer
      title="研究协议卡"
      width="min(100vw, 640px)"
      open={open}
      onClose={onClose}
      destroyOnHidden
      extra={(
        <Button onClick={() => form.setFieldsValue(DEMO_PROTOCOL_TEMPLATE)} disabled={saving || readOnly}>
          加载演示协议
        </Button>
      )}
      footer={(
        <div className="research-protocol-drawer__footer">
          <Text type="secondary">
            {readOnly ? '该会话已归档，研究协议仅供查看。' : '草稿可继续编辑；审批后才能运行正式分析方案。'}
          </Text>
          {!readOnly ? (
            <Space>
              <Button onClick={() => void save('Draft')} loading={saving}>保存草稿</Button>
              <Button type="primary" onClick={() => void save('Approved')} loading={saving}>
                审批协议
              </Button>
            </Space>
          ) : null}
        </div>
      )}
    >
      <Paragraph type="secondary">
        用结构化协议固定研究问题、时间零点、估计目标与分析策略；系统仅提供科研统计辅助，
        不替代临床诊疗或研究者最终审核。
      </Paragraph>
      {protocol?.status === 'Approved' ? (
        <Alert type="success" showIcon message={`协议 v${protocol.version} 已由服务端审批 · ${protocol.approved_at ?? ''}`} />
      ) : null}
      {error ? <Alert type="error" showIcon message={error} style={{ marginTop: 12 }} /> : null}

      <Form form={form} layout="vertical" className="research-protocol-form" disabled={readOnly}>
        <Form.Item name="research_question" label="1. 研究问题" rules={requiredRule}>
          <Input.TextArea autoSize={{ minRows: 2, maxRows: 4 }} />
        </Form.Item>
        <Form.Item name="study_design" label="2. 研究设计">
          <Select options={[
            { value: 'cross_sectional', label: '横断面研究' },
            { value: 'cohort', label: '队列研究' },
            { value: 'case_control', label: '病例对照研究' },
            { value: 'randomized_trial', label: '随机试验' },
            { value: 'other', label: '其他' },
          ]} />
        </Form.Item>
        <Form.Item name="population" label="3. 目标人群" rules={requiredRule}><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="eligibility_criteria" label="4. 纳入 / 排除标准"><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="exposure" label="5. 暴露 / 干预"><Input /></Form.Item>
        <Form.Item name="comparator" label="6. 对照"><Input /></Form.Item>
        <Form.Item name="outcome" label="7. 结局" rules={requiredRule}><Input /></Form.Item>
        <Form.Item name="time_zero" label="8. 时间零点" rules={requiredRule}><Input /></Form.Item>
        <Form.Item name="follow_up" label="9. 随访窗口"><Input /></Form.Item>
        <Form.Item name="analysis_unit" label="10. 分析单位" rules={requiredRule}><Input /></Form.Item>
        <Form.Item name="estimand" label="11. 目标估计量（estimand）" rules={requiredRule}><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="confounders" label="12. 混杂因素 / 中介 / 碰撞变量"><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="missing_data_strategy" label="13. 缺失数据策略"><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="primary_analysis" label="14. 主分析" rules={requiredRule}><Input.TextArea autoSize /></Form.Item>
        <Form.Item name="sensitivity_analysis" label="15. 敏感性分析"><Input.TextArea autoSize /></Form.Item>
      </Form>
    </Drawer>
  );
}

export default ResearchProtocolDrawer;
