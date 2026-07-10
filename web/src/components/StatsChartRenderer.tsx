import { useMemo } from 'react';
import ReactECharts from 'echarts-for-react';
import { Card, Typography, Space, Tag } from 'antd';
import { AreaChartOutlined } from '@ant-design/icons';
import type { SkillResult } from '../api/types';
import { fmtNum, fmtP, normalizeCoefficients } from '../lib/coeffFields';

const { Text } = Typography;

export interface StatsChartRendererProps {
  skillResult: SkillResult | null;
}

export function StatsChartRenderer({ skillResult }: StatsChartRendererProps) {
  const chartOption = useMemo(() => {
    if (!skillResult || !skillResult.payload) return null;

    const payload = skillResult.payload as any;

    // ─── Scenario 1: Kaplan-Meier Survival Analysis (SurvivalKmResult) ───
    if (payload.steps && Array.isArray(payload.steps)) {
      const steps = payload.steps;
      const groups = payload.groups && payload.groups.length > 0 ? payload.groups : ['Overall'];

      const series: any[] = [];
      const colors = ['#38618c', '#5f87b3', '#d97706', '#059669', '#7c3aed'];

      groups.forEach((group: string, gIdx: number) => {
        const groupSteps = steps.filter((s: any) => s.group === group);
        // Sort by follow-up time
        groupSteps.sort((a: any, b: any) => a.time - b.time);

        // Standard K-M curve starts at time=0, survival=1.0
        const data = [[0, 1.0]];
        groupSteps.forEach((s: any) => {
          data.push([s.time, s.survival]);
        });

        series.push({
          name: group,
          type: 'line',
          step: 'end',
          data: data,
          symbol: 'circle',
          symbolSize: 4,
          lineStyle: {
            width: 2,
            color: colors[gIdx % colors.length],
          },
          itemStyle: {
            color: colors[gIdx % colors.length],
          },
        });
      });

      const pValueText = payload.log_rank
        ? `Log-rank p: ${payload.log_rank.p_value < 0.001 ? '< 0.001' : payload.log_rank.p_value.toFixed(4)}`
        : '';

      return {
        title: {
          text: 'Kaplan-Meier 生存曲线',
          subtext: pValueText,
          left: 'center',
          textStyle: { color: '#2b3a4a', fontSize: 14, fontWeight: 'bold' },
        },
        tooltip: {
          trigger: 'axis',
          formatter: (params: any) => {
            let res = `随访时间: ${params[0].value[0]} 天<br/>`;
            params.forEach((p: any) => {
              res += `${p.marker} ${p.seriesName}: ${(p.value[1] * 100).toFixed(1)}% 生存率<br/>`;
            });
            return res;
          },
        },
        legend: {
          bottom: 0,
          data: groups,
        },
        grid: {
          top: 60,
          left: '5%',
          right: '5%',
          bottom: 40,
          containLabel: true,
        },
        xAxis: {
          type: 'value',
          name: '时间 (随随访时间/天)',
          nameLocation: 'middle',
          nameGap: 25,
          splitLine: { show: false },
          axisLine: { lineStyle: { color: '#8ca5c0' } },
        },
        yAxis: {
          type: 'value',
          name: '生存累积概率 (Cumulative Survival)',
          nameLocation: 'middle',
          nameGap: 35,
          min: 0,
          max: 1.0,
          splitLine: { lineStyle: { type: 'dashed', color: '#e2e8f0' } },
          axisLine: { lineStyle: { color: '#8ca5c0' } },
        },
        series: series,
      };
    }

    // ─── Scenario 2: Regression Forest Plot (LogisticResult / CoxResult / LinearResult) ───
    if (payload.coefficients && Array.isArray(payload.coefficients)) {
      const coefficients = normalizeCoefficients(payload.coefficients);
      if (coefficients.length === 0) return null;

      const isLogistic = coefficients.some((c) => c.oddsRatio !== null);
      const isCox = !isLogistic && coefficients.some((c) => c.hazardRatio !== null);

      // Extract names, estimates, and 95% Confidence Intervals (skip incomplete rows)
      const categories: string[] = [];
      const pointEstimates: number[] = [];
      const errorBarData: any[] = [];
      const plotCoeffs = coefficients.filter((coeff) => {
        const est = isLogistic ? coeff.oddsRatio : isCox ? coeff.hazardRatio : coeff.beta;
        return est !== null && coeff.ciLower !== null && coeff.ciUpper !== null;
      });

      plotCoeffs.forEach((coeff) => {
        categories.push(coeff.term);
        const est = (isLogistic ? coeff.oddsRatio : isCox ? coeff.hazardRatio : coeff.beta) as number;
        pointEstimates.push(est);
        errorBarData.push([est, categories.length - 1, coeff.ciLower, coeff.ciUpper]);
      });

      if (categories.length === 0) return null;

      // Reverse lists to render top-down in forest plot
      categories.reverse();
      pointEstimates.reverse();
      errorBarData.forEach((item) => {
        item[1] = categories.length - 1 - item[1]; // Adjust index
      });

      const refLineVal = isLogistic || isCox ? 1.0 : 0.0;
      const xLabel = isLogistic ? '比值比 (Odds Ratio, OR)' : isCox ? '风险比 (Hazard Ratio, HR)' : '回归系数 (Beta)';

      return {
        title: {
          text: `${isLogistic ? 'Logistic' : isCox ? 'Cox 比例风险' : '线性'} 回归效应森林图 (Forest Plot)`,
          left: 'center',
          textStyle: { color: '#2b3a4a', fontSize: 14, fontWeight: 'bold' },
        },
        tooltip: {
          trigger: 'axis',
          axisPointer: { type: 'shadow' },
          formatter: (params: any) => {
            const scatterParam = params.find((p: any) => p.seriesName === '点估计');
            if (!scatterParam) return '';
            const idx = scatterParam.dataIndex;
            const coeff = plotCoeffs[plotCoeffs.length - 1 - idx];
            if (!coeff) return '';
            const est = isLogistic ? coeff.oddsRatio : isCox ? coeff.hazardRatio : coeff.beta;
            return `<b>${coeff.term}</b><br/>
                    效应值: ${fmtNum(est)}<br/>
                    95% 置信区间: [${fmtNum(coeff.ciLower)}, ${fmtNum(coeff.ciUpper)}]<br/>
                    p 值: ${fmtP(coeff.pValue, 4)}`;
          },
        },
        grid: {
          top: 60,
          left: '15%',
          right: '8%',
          bottom: 50,
          containLabel: true,
        },
        xAxis: {
          type: 'value',
          name: xLabel,
          nameLocation: 'middle',
          nameGap: 30,
          splitLine: { lineStyle: { type: 'dashed', color: '#e2e8f0' } },
          axisLine: { lineStyle: { color: '#8ca5c0' } },
        },
        yAxis: {
          type: 'category',
          data: categories,
          axisLine: { lineStyle: { color: '#8ca5c0' } },
          axisLabel: { fontWeight: 'bold' },
        },
        series: [
          // 1. Reference line at 1.0 (or 0.0)
          {
            type: 'line',
            markLine: {
              symbol: 'none',
              lineStyle: { type: 'dashed', color: '#dc2626', width: 1.5 },
              data: [{ xAxis: refLineVal }],
            },
          },
          // 2. Custom series for Confidence Intervals
          {
            type: 'custom',
            name: '置信区间',
            renderItem: (_params: any, api: any) => {
              const yIndex = api.value(1);
              const low = api.value(2);
              const high = api.value(3);
              const halfWidth = 5;

              const startCoords = api.coord([low, yIndex]);
              const endCoords = api.coord([high, yIndex]);

              const style = { stroke: '#38618c', lineWidth: 1.8 };

              return {
                type: 'group',
                children: [
                  {
                    type: 'line',
                    shape: {
                      x1: startCoords[0],
                      y1: startCoords[1],
                      x2: endCoords[0],
                      y2: endCoords[1],
                    },
                    style: style,
                  },
                  {
                    type: 'line',
                    shape: {
                      x1: startCoords[0],
                      y1: startCoords[1] - halfWidth,
                      x2: startCoords[0],
                      y2: startCoords[1] + halfWidth,
                    },
                    style: style,
                  },
                  {
                    type: 'line',
                    shape: {
                      x1: endCoords[0],
                      y1: endCoords[1] - halfWidth,
                      x2: endCoords[0],
                      y2: endCoords[1] + halfWidth,
                    },
                    style: style,
                  },
                ],
              };
            },
            data: errorBarData,
          },
          // 3. Point Estimates (Scatter marker)
          {
            name: '点估计',
            type: 'scatter',
            symbol: isLogistic || isCox ? 'roundRect' : 'circle',
            symbolSize: 10,
            itemStyle: {
              color: '#38618c',
              borderColor: '#ffffff',
              borderWidth: 1,
            },
            data: pointEstimates,
          },
        ],
      };
    }

    // ─── Scenario 3: One-Way ANOVA / T-Test / Variance Homogeneity ───
    if (payload.groups && Array.isArray(payload.groups) && 'overall_mean' in payload) {
      // ANOVA groups summary
      const groupData = payload.groups as any[];
      const names = groupData.map((g) => g.group);
      const means = groupData.map((g) => g.mean);
      const sds = groupData.map((g) => g.sd);

      const errorBarData = groupData.map((g, idx) => {
        return [idx, g.mean - g.sd, g.mean + g.sd];
      });

      return {
        title: {
          text: `组间均值对比图 (${payload.variable || '指标'})`,
          subtext: `ANOVA p-value: ${payload.p_value < 0.001 ? '<0.001' : payload.p_value.toFixed(4)}`,
          left: 'center',
          textStyle: { color: '#2b3a4a', fontSize: 14, fontWeight: 'bold' },
        },
        tooltip: {
          trigger: 'axis',
          formatter: (params: any) => {
            const param = params.find((p: any) => p.seriesName === '组均值');
            if (!param) return '';
            const idx = param.dataIndex;
            return `<b>${names[idx]}</b><br/>
                    均值 (Mean): ${means[idx].toFixed(3)}<br/>
                    标准差 (SD): ±${sds[idx].toFixed(3)}`;
          },
        },
        grid: {
          top: 65,
          left: '8%',
          right: '5%',
          bottom: 40,
          containLabel: true,
        },
        xAxis: {
          type: 'category',
          data: names,
          axisLine: { lineStyle: { color: '#8ca5c0' } },
        },
        yAxis: {
          type: 'value',
          name: '均值 (Mean)',
          splitLine: { lineStyle: { type: 'dashed', color: '#e2e8f0' } },
          axisLine: { lineStyle: { color: '#8ca5c0' } },
        },
        series: [
          // 1. Column Bars
          {
            name: '组均值',
            type: 'bar',
            data: means,
            barWidth: '35%',
            itemStyle: {
              color: '#38618c',
              borderRadius: [4, 4, 0, 0],
            },
          },
          // 2. Custom vertical error bars
          {
            type: 'custom',
            name: '标准差 (Mean ± SD)',
            renderItem: (_params: any, api: any) => {
              const xIdx = api.value(0);
              const low = api.value(1);
              const high = api.value(2);
              const halfWidth = 6;

              const lowCoords = api.coord([xIdx, low]);
              const highCoords = api.coord([xIdx, high]);

              const style = { stroke: '#e28743', lineWidth: 1.5 };

              return {
                type: 'group',
                children: [
                  {
                    type: 'line',
                    shape: { x1: lowCoords[0], y1: lowCoords[1], x2: highCoords[0], y2: highCoords[1] },
                    style: style,
                  },
                  {
                    type: 'line',
                    shape: { x1: lowCoords[0] - halfWidth, y1: lowCoords[1], x2: lowCoords[0] + halfWidth, y2: lowCoords[1] },
                    style: style,
                  },
                  {
                    type: 'line',
                    shape: { x1: highCoords[0] - halfWidth, y1: highCoords[1], x2: highCoords[0] + halfWidth, y2: highCoords[1] },
                    style: style,
                  },
                ],
              };
            },
            data: errorBarData,
          },
        ],
      };
    }

    // ─── Scenario 4: Power Analysis (PowerResult) ───
    if (payload.method && (payload.power !== undefined || payload.total_n !== undefined)) {
      const powerVal = payload.power || 0.8;
      return {
        title: {
          text: `检验功效分析 (Power Analysis: ${payload.method})`,
          left: 'center',
          textStyle: { color: '#2b3a4a', fontSize: 14, fontWeight: 'bold' },
        },
        series: [
          {
            type: 'gauge',
            center: ['50%', '60%'],
            startAngle: 200,
            endAngle: -20,
            min: 0,
            max: 1.0,
            splitNumber: 5,
            axisLine: {
              lineStyle: {
                width: 8,
                color: [
                  [0.8, '#ff4d4f'], // low power < 0.8 is red
                  [1.0, '#52c41a'], // sufficient power is green
                ],
              },
            },
            pointer: {
              icon: 'path://M12.8,0.7l12,20.1c0.6,1,0.6,2.3,0,3.3c-0.6,1-1.7,1.6-2.9,1.6H1.7c-1.2,0-2.3-0.6-2.9-1.6c-0.6-1-0.6-2.3,0-3.3l12-20.1C11.3-0.2,12.2-0.2,12.8,0.7z',
              width: 8,
              length: '65%',
              offsetCenter: [0, '8%'],
              itemStyle: { color: '#38618c' },
            },
            axisTick: { distance: -16, splitNumber: 5, lineStyle: { width: 1, color: '#999' } },
            splitLine: { distance: -20, length: 10, lineStyle: { width: 2, color: '#999' } },
            axisLabel: {
              distance: -10,
              color: '#4b5563',
              fontSize: 10,
              formatter: (val: number) => val.toFixed(1),
            },
            anchor: {
              show: true,
              showAbove: true,
              size: 14,
              itemStyle: { color: '#38618c' },
            },
            title: { show: false },
            detail: {
              valueAnimation: true,
              width: '60%',
              lineHeight: 40,
              borderRadius: 8,
              offsetCenter: [0, '40%'],
              fontSize: 22,
              fontWeight: 'bold',
              formatter: (val: number) => `功效 (Power): ${(val * 100).toFixed(1)}%`,
              color: '#38618c',
            },
            data: [{ value: powerVal }],
          },
        ],
      };
    }

    return null;
  }, [skillResult]);

  if (!chartOption) {
    return null;
  }

  return (
    <Card
      className="glass-panel"
      title={
        <Space size={6}>
          <AreaChartOutlined style={{ color: '#38618c' }} />
          <Text strong>学术级交互图表 (ECharts)</Text>
          <Tag color="success" style={{ margin: 0 }}>
            可交互
          </Tag>
        </Space>
      }
      styles={{ body: { padding: '16px' } }}
      style={{ marginTop: 12 }}
    >
      <ReactECharts
        option={chartOption}
        style={{ height: '350px', width: '100%' }}
        opts={{ renderer: 'canvas' }}
      />
    </Card>
  );
}

export default StatsChartRenderer;
