import { useState, useEffect } from 'react';
import { Card, Steps, Typography, Space } from 'antd';
import {
  LoadingOutlined,
  CheckCircleOutlined,
  PlayCircleOutlined,
  DeploymentUnitOutlined,
} from '@ant-design/icons';
import type { ConnectionStatus } from '../hooks/useSseChat';

const { Text, Title } = Typography;

export interface EngineRunnerProps {
  status: ConnectionStatus;
  isStreaming: boolean;
  logs?: string[]; // Optional background logs or interpretation text
}

export function EngineRunner({ status, isStreaming, logs = [] }: EngineRunnerProps) {
  const [currentStep, setCurrentStep] = useState(0);

  // Automatically advance steps based on connection/streaming states
  useEffect(() => {
    if (status === 'connecting') {
      setCurrentStep(0);
    } else if (status === 'streaming') {
      // If we start streaming, advance to data processing or model fitting
      setCurrentStep(1);
      // Simulate moving to model fitting after a short duration in stream
      const timer = setTimeout(() => {
        setCurrentStep(2);
      }, 3500);

      const timer2 = setTimeout(() => {
        setCurrentStep(3);
      }, 7000);

      return () => {
        clearTimeout(timer);
        clearTimeout(timer2);
      };
    } else if (status === 'idle' && !isStreaming) {
      setCurrentStep(4);
    }
  }, [status, isStreaming]);

  const stepItems = [
    {
      title: '连接分析引擎',
      description: currentStep > 0 ? '已连接' : '正在对接服务...',
    },
    {
      title: '数据读取与清洗',
      description: currentStep > 1 ? '已校验' : currentStep === 1 ? '正在检查变量分布...' : '等待中',
    },
    {
      title: '模型演算与拟合',
      description: currentStep > 2 ? '已收敛' : currentStep === 2 ? '统计模型训练拟合中...' : '等待中',
    },
    {
      title: '生成学术报告',
      description: currentStep > 3 ? '已生成' : currentStep === 3 ? '正在生成学术三线表与图表...' : '等待中',
    },
  ];

  // Resolve icon based on status
  const renderStepIcon = (index: number) => {
    if (index < currentStep) {
      return <CheckCircleOutlined style={{ color: '#52c41a' }} />;
    }
    if (index === currentStep) {
      return <LoadingOutlined style={{ color: '#38618c' }} spin />;
    }
    return <PlayCircleOutlined style={{ color: '#bfbfbf' }} />;
  };

  return (
    <Card className="glass-panel" style={{ padding: '24px 12px' }}>
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', width: '100%' }}>
        {/* Animated outer glowing ring & pulse */}
        <div
          style={{
            position: 'relative',
            width: '120px',
            height: '120px',
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            marginBottom: '24px',
          }}
        >
          {/* Pulsing glow background */}
          <div
            className="engine-pulse"
            style={{
              position: 'absolute',
              width: '100%',
              height: '100%',
            }}
          />

          {/* Slow spinning dashed element */}
          <div
            className="engine-spin-slow"
            style={{
              position: 'absolute',
              width: '90px',
              height: '90px',
              borderRadius: '50%',
              border: '2px dashed rgba(56, 97, 140, 0.4)',
            }}
          />

          {/* Centered statistical computation icon */}
          <div
            style={{
              position: 'absolute',
              width: '60px',
              height: '60px',
              borderRadius: '50%',
              background: '#38618c',
              display: 'flex',
              justifyContent: 'center',
              alignItems: 'center',
              boxShadow: '0 4px 12px rgba(56, 97, 140, 0.3)',
            }}
          >
            <DeploymentUnitOutlined style={{ fontSize: '28px', color: '#ffffff' }} />
          </div>
        </div>

        <Title level={4} style={{ margin: '0 0 8px 0', color: '#2b3a4a', textAlign: 'center' }}>
          {status === 'connecting'
            ? '正在建立云端/本地统计连接'
            : status === 'streaming'
            ? '统计引擎加速运行中...'
            : status === 'error'
            ? '计算发生异常中断'
            : '运算处理完成'}
        </Title>

        <Text type="secondary" style={{ fontSize: '13px', marginBottom: '32px', textAlign: 'center', maxWidth: '400px' }}>
          正在加载高级计算包。我们将自动验证数学假设条件，并构建包含 P 值、置信区间及效应量的统计模型。
        </Text>

        {/* Formatted statistical progress steps */}
        <div style={{ width: '100%', maxWidth: '640px', marginBottom: '24px' }}>
          <Steps
            current={currentStep}
            size="small"
            items={stepItems.map((item, idx) => ({
              title: <span style={{ fontWeight: idx === currentStep ? 600 : 400 }}>{item.title}</span>,
              description: <span style={{ fontSize: '12px' }}>{item.description}</span>,
              icon: renderStepIcon(idx),
            }))}
          />
        </div>

        {/* Realtime Stream Excerpts Log */}
        {logs.length > 0 && (
          <div
            style={{
              width: '100%',
              maxWidth: '640px',
              background: 'rgba(56, 97, 140, 0.03)',
              border: '1px solid rgba(56, 97, 140, 0.08)',
              borderRadius: '8px',
              padding: '12px 16px',
              maxHeight: '120px',
              overflowY: 'auto',
            }}
          >
            <Space direction="vertical" size={2} style={{ width: '100%' }}>
              <Text strong style={{ fontSize: '11px', color: '#38618c', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                计算日志流 (Execution Logs)
              </Text>
              {logs.slice(-3).map((log, idx) => (
                <div key={idx} style={{ display: 'flex', gap: '8px', fontSize: '12px' }}>
                  <span style={{ color: '#8ca5c0' }}>&gt;</span>
                  <Text code style={{ background: 'transparent', border: 'none', padding: 0, color: '#4a5b6d' }}>
                    {log}
                  </Text>
                </div>
              ))}
            </Space>
          </div>
        )}
      </div>
    </Card>
  );
}

export default EngineRunner;
