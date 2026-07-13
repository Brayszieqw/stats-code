import { useState, useMemo, useEffect } from 'react';
import { Form, Select, Button, Card, Radio, Space, Typography, Tooltip, Row, Col } from 'antd';
import {
  SettingOutlined,
  PlayCircleOutlined,
  QuestionCircleOutlined,
} from '@ant-design/icons';
import type { DatasetSummary, RunRequest } from '../api/types';

const { Text, Title, Paragraph } = Typography;

export interface AnalysisConfiguratorProps {
  summary: DatasetSummary | null;
  onSubmit: (request: RunRequest, promptText: string) => void;
  disabled?: boolean;
}

type AnalysisType = 'table_one' | 't_test' | 'survival' | 'regression';

export function AnalysisConfigurator({ summary, onSubmit, disabled }: AnalysisConfiguratorProps) {
  const [form] = Form.useForm();
  const [analysisType, setAnalysisType] = useState<AnalysisType>('table_one');
  const [regressionType, setRegressionType] = useState<'linear' | 'logistic' | 'cox'>('linear');

  // Set default values when dataset changes
  useEffect(() => {
    form.resetFields();
  }, [summary, form]);

  const allColumns = useMemo(() => {
    if (!summary) return [];
    return summary.columns.map((c) => ({
      label: `${c.name} (${c.inferred_type})`,
      value: c.name,
      type: c.inferred_type,
    }));
  }, [summary]);

  const numericColumns = useMemo(() => {
    return allColumns.filter((c) => c.type === 'Numeric');
  }, [allColumns]);

  const categoricalColumns = useMemo(() => {
    return allColumns.filter((c) => c.type === 'Categorical' || c.type === 'String');
  }, [allColumns]);

  const handleFinish = (values: any) => {
    if (!summary) return;

    let prompt = '';
    let request: RunRequest | null = null;
    const fileName = summary.file_name;

    switch (analysisType) {
      case 'table_one': {
        const { group, continuous, categorical } = values;
        prompt = `请根据数据集 "${fileName}" 生成学术规范的基线特征表 (Table One)。\n`;
        if (group) {
          prompt += `- 分组变量为 "${group}"。\n`;
        }
        if (continuous && continuous.length > 0) {
          prompt += `- 连续型数值变量包括: ${continuous.map((c: string) => `"${c}"`).join(', ')}，请采用均值±标准差或中位数(四分位距)描述，并执行组间差异性检验。\n`;
        }
        if (categorical && categorical.length > 0) {
          prompt += `- 分类变量包括: ${categorical.map((c: string) => `"${c}"`).join(', ')}，请采用频数(百分比)描述，并执行卡方检验或 Fisher 精确检验。\n`;
        }
        prompt += `请输出符合 APA 期刊发表标准的三线表格式。`;
        request = {
          skill_id: 'tableone',
          dataset_id: summary.dataset_id,
          args: {
            ...(group ? { group } : {}),
            continuous: continuous ?? [],
            categorical: categorical ?? [],
          },
        };
        break;
      }
      case 't_test': {
        const { group, testVar } = values;
        prompt = `请对数据集 "${fileName}" 执行双独立样本 T 检验 (Independent Samples T-Test)。\n`;
        prompt += `- 分组自变量为 "${group}"（应包含两个分组类别）。\n`;
        prompt += `- 待检验因变量为 "${testVar}"（数值型连续变量）。\n`;
        prompt += `请计算两组的均值、标准差、t 统计量、自由度、双侧 p 值、95% 置信区间以及效应量 Cohen's d。请同时检查方差齐性 (Levene's Test)。请生成对比箱线图的数据和学术三线表。`;
        request = {
          skill_id: 'ttest',
          dataset_id: summary.dataset_id,
          args: { group, testVar },
        };
        break;
      }
      case 'survival': {
        const { time, status, group } = values;
        prompt = `请对数据集 "${fileName}" 进行 Kaplan-Meier 生存分析。\n`;
        prompt += `- 生存时间变量为 "${time}"。\n`;
        prompt += `- 删失状态变量为 "${status}"（用 1 表示终点事件发生，0 表示删失）。\n`;
        if (group) {
          prompt += `- 分组比较变量为 "${group}"。\n`;
        }
        prompt += `请计算中位生存时间、各时间节点的生存率、Log-rank 检验统计量与 p 值，并输出 Kaplan-Meier 生存曲线数据与学术生存表。`;
        request = {
          skill_id: 'survival_km',
          dataset_id: summary.dataset_id,
          args: { time, event: status, ...(group ? { group } : {}) },
        };
        break;
      }
      case 'regression': {
        const { dependent, independent, timeVar } = values;
        const modelType = values.modelType ?? regressionType;
        if (modelType === 'cox') {
          prompt = `请对数据集 "${fileName}" 构建 Cox 比例风险回归模型 (Cox Proportional Hazards Model)。\n`;
          prompt += `- 时间变量为 "${timeVar}"，终点事件变量为 "${dependent}"。\n`;
          request = {
            skill_id: 'model_cox',
            dataset_id: summary.dataset_id,
            args: { time: timeVar, event: dependent, predictors: independent },
          };
        } else {
          const typeLabel = modelType === 'logistic' ? 'Logistic' : '多元线性';
          prompt = `请对数据集 "${fileName}" 构建 ${typeLabel} 回归模型 (Regression Model)。\n`;
          prompt += `- 因变量 (Y) 为 "${dependent}"。\n`;
          request = {
            skill_id: modelType === 'logistic' ? 'model_logistic' : 'model_linear',
            dataset_id: summary.dataset_id,
            args: { outcome: dependent, predictors: independent },
          };
        }
        prompt += `- 自变量 (X) 包括: ${independent.map((c: string) => `"${c}"`).join(', ')}。\n`;
        prompt += `请输出回归系数 Beta 值、标准误 SE、p 值、${modelType === 'logistic' ? '比值比 OR' : modelType === 'cox' ? '风险比 HR' : '标准化系数'}、95% 置信区间以及模型拟合度评估 (如 R²、AIC)。同时输出自变量回归效应森林图所需的数据。`;
        break;
      }
    }

    if (request) {
      onSubmit(request, prompt);
    }
  };

  if (!summary) {
    return null;
  }

  return (
    <Card
      className="glass-panel analysis-configurator-card"
      title={
        <Title level={5} style={{ margin: 0, color: '#2b3a4a' }}>
          <SettingOutlined style={{ marginRight: '6px', color: '#38618c' }} />
          可视化统计分析配置
        </Title>
      }
      styles={{ body: { padding: '20px' } }}
    >
      <Paragraph style={{ fontSize: '13px', color: '#5a6e85', marginBottom: '16px' }}>
        选择统计模块并配置变量。系统会生成结构化分析方案，经您审批后提交本机确定性引擎。
      </Paragraph>

      <Form
        form={form}
        layout="vertical"
        onFinish={handleFinish}
        disabled={disabled}
        initialValues={{
          analysis_type: 'table_one',
          modelType: 'linear',
        }}
      >
        {/* Step 1: Select Analysis Type */}
        <Form.Item name="analysis_type" label={<Text strong>分析模块类型</Text>}>
          <Radio.Group
            optionType="button"
            buttonStyle="solid"
            onChange={(e) => setAnalysisType(e.target.value)}
            style={{ width: '100%', display: 'flex' }}
          >
            <Radio.Button value="table_one" style={{ flex: 1, textAlign: 'center' }}>
              基线特征 (TableOne)
            </Radio.Button>
            <Radio.Button value="t_test" style={{ flex: 1, textAlign: 'center' }}>
              T检验组间对比
            </Radio.Button>
            <Radio.Button value="survival" style={{ flex: 1, textAlign: 'center' }}>
              KM生存分析
            </Radio.Button>
            <Radio.Button value="regression" style={{ flex: 1, textAlign: 'center' }}>
              回归建模分析
            </Radio.Button>
          </Radio.Group>
        </Form.Item>

        <Row gutter={24}>
          <Col span={24}>
            {/* TableOne Config */}
            {analysisType === 'table_one' && (
              <>
                <Form.Item
                  name="group"
                  label={
                    <Space size={4}>
                      <Text strong>分组比较变量 (Strata)</Text>
                      <Tooltip title="选填。根据该分类变量（如：治疗组/对照组）进行亚组对比与差异性检验。不选则展示全人群描述。">
                        <QuestionCircleOutlined style={{ color: '#8c8c8c' }} />
                      </Tooltip>
                    </Space>
                  }
                >
                  <Select
                    placeholder="请选择分组变量 (如 Group, Sex等)"
                    options={allColumns}
                    allowClear
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="continuous"
                  label={<Text strong>连续性数值变量 (Continuous Variables)</Text>}
                  rules={[{ required: true, message: '请至少选择一个数值型变量' }]}
                >
                  <Select
                    mode="multiple"
                    placeholder="选择用于计算均值/中位数的指标 (如 Age, BMI等)"
                    options={numericColumns}
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="categorical"
                  label={<Text strong>分类指示变量 (Categorical Variables)</Text>}
                >
                  <Select
                    mode="multiple"
                    placeholder="选择用于计算频数与率的指标 (如 Stage, Race等)"
                    options={categoricalColumns}
                    showSearch
                  />
                </Form.Item>
              </>
            )}

            {/* T-Test Config */}
            {analysisType === 't_test' && (
              <>
                <Form.Item
                  name="group"
                  label={
                    <Space size={4}>
                      <Text strong>分组自变量 (Grouping Variable)</Text>
                      <Tooltip title="必填。必须是含有且仅含有两个类别的分类/文本型变量。">
                        <QuestionCircleOutlined style={{ color: '#8c8c8c' }} />
                      </Tooltip>
                    </Space>
                  }
                  rules={[{ required: true, message: '请选择分组变量' }]}
                >
                  <Select
                    placeholder="请选择分类自变量 (两组，如 Gender: 0/1 或 Arm: A/B)"
                    options={allColumns}
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="testVar"
                  label={<Text strong>待检验因变量 (Test Variable)</Text>}
                  rules={[{ required: true, message: '请选择数值测试因变量' }]}
                >
                  <Select
                    placeholder="请选择连续数值型变量 (如 Score, BloodPressure等)"
                    options={numericColumns}
                    showSearch
                  />
                </Form.Item>
              </>
            )}

            {/* Survival Analysis Config */}
            {analysisType === 'survival' && (
              <>
                <Form.Item
                  name="time"
                  label={<Text strong>生存时间变量 (Survival Time)</Text>}
                  rules={[{ required: true, message: '请选择时间变量' }]}
                >
                  <Select
                    placeholder="表示随访时长的连续变量 (如 Days, Months等)"
                    options={numericColumns}
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="status"
                  label={
                    <Space size={4}>
                      <Text strong>终点事件 / 删失状态 (Event / Status)</Text>
                      <Tooltip title="必填。0 表示生存或删失，1 表示终点事件发生（如死亡/复发）。">
                        <QuestionCircleOutlined style={{ color: '#8c8c8c' }} />
                      </Tooltip>
                    </Space>
                  }
                  rules={[{ required: true, message: '请选择状态变量' }]}
                >
                  <Select
                    placeholder="生存事件指示变量 (0 = 删失, 1 = 终点事件)"
                    options={numericColumns}
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="group"
                  label={
                    <Space size={4}>
                      <Text strong>生存比较分组变量 (Strata Factor)</Text>
                      <Tooltip title="选填。根据该变量生成多条 Kaplan-Meier 曲线并执行 Log-rank 组间显著性比较。">
                        <QuestionCircleOutlined style={{ color: '#8c8c8c' }} />
                      </Tooltip>
                    </Space>
                  }
                >
                  <Select
                    placeholder="请选择分组比较变量 (如 Group, Stage等)"
                    options={allColumns}
                    allowClear
                    showSearch
                  />
                </Form.Item>
              </>
            )}

            {/* Regression Config */}
            {analysisType === 'regression' && (
              <>
                <Form.Item name="modelType" label={<Text strong>回归模型分类</Text>}>
                  <Radio.Group
                    onChange={(e) => setRegressionType(e.target.value)}
                    value={regressionType}
                  >
                    <Radio value="linear">多元线性回归 (连续型Y)</Radio>
                    <Radio value="logistic">Logistic 回归 (二分类Y)</Radio>
                    <Radio value="cox">Cox 比例风险回归 (生存Y)</Radio>
                  </Radio.Group>
                </Form.Item>

                {regressionType === 'cox' && (
                  <Form.Item
                    name="timeVar"
                    label={<Text strong>随访时间变量 (Time)</Text>}
                    rules={[{ required: true, message: '请选择随访时间变量' }]}
                  >
                    <Select
                      placeholder="请选择表示生存随访时间的变量"
                      options={numericColumns}
                      showSearch
                    />
                  </Form.Item>
                )}

                <Form.Item
                  name="dependent"
                  label={
                    <Text strong>
                      {regressionType === 'cox' ? '终点事件变量 (Event)' : '因变量 (Dependent Variable Y)'}
                    </Text>
                  }
                  rules={[{ required: true, message: '请选择因变量/事件变量' }]}
                >
                  <Select
                    placeholder={
                      regressionType === 'logistic'
                        ? '请选择二分类因变量 (如 Status: 0/1)'
                        : regressionType === 'cox'
                        ? '请选择生存结局变量 (0=删失, 1=发生)'
                        : '请选择连续型因变量 Y'
                    }
                    options={regressionType === 'linear' ? numericColumns : allColumns}
                    showSearch
                  />
                </Form.Item>

                <Form.Item
                  name="independent"
                  label={<Text strong>自变量列表 (Independent Variables X)</Text>}
                  rules={[{ required: true, message: '请选择至少一个自变量' }]}
                >
                  <Select
                    mode="multiple"
                    placeholder="请选择进入回归模型的协变量/自变量"
                    options={allColumns}
                    showSearch
                  />
                </Form.Item>
              </>
            )}
          </Col>
        </Row>

        <Form.Item style={{ marginBottom: 0, marginTop: '12px' }}>
          <Button
            className="analysis-configurator-submit"
            type="primary"
            htmlType="submit"
            icon={<PlayCircleOutlined />}
            size="large"
            block
            style={{
              height: '42px',
              borderRadius: '7px',
              background: '#244f73',
              border: 'none',
              boxShadow: '0 10px 24px -16px rgba(23, 59, 90, 0.9)',
            }}
          >
            开始统计计算
          </Button>
        </Form.Item>
      </Form>
    </Card>
  );
}

export default AnalysisConfigurator;
