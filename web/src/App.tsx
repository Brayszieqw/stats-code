import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ConfigProvider, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { CoverageMatrixProvider } from './lib/coverageMatrixContext';
import { AppShell } from './AppShell';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

/**
 * Custom Ant Design theme — 「学术期刊」视觉系统。
 *
 * 与 index.css 的 CSS 变量同源：学术墨蓝主色、暖纸底色、朱砂点缀。
 * 标题走思源宋体（serif-display 工具类），正文保持无衬线以保证可读性。
 */
const customTheme = {
  algorithm: theme.defaultAlgorithm,
  token: {
    colorPrimary: '#38618c', // 学术墨蓝
    colorInfo: '#38618c',
    colorError: '#c0392b', // 朱砂红（与批注色一致）
    colorTextBase: '#2b3a4a',
    borderRadius: 10,
    borderRadiusLG: 14,
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", "PingFang SC", "Microsoft YaHei", "Source Han Sans CN", sans-serif',
    fontSize: 14,
    colorBgLayout: '#f7f5ef', // 暖纸底色
    colorBorderSecondary: '#e9e6dd',
    boxShadowTertiary:
      '0 1px 2px 0 rgba(31, 43, 56, 0.03), 0 1px 6px -1px rgba(31, 43, 56, 0.02), 0 2px 4px 0 rgba(31, 43, 56, 0.02)',
  },
  components: {
    Layout: {
      headerBg: 'rgba(250, 249, 245, 0.85)',
      bodyBg: 'transparent', // 允许透出底层的 CSS 渐变色
      siderBg: 'rgba(250, 249, 245, 0.85)',
    },
    Card: {
      borderRadiusLG: 14,
      colorBorderSecondary: '#e9e6dd',
    },
    Button: {
      controlHeight: 36,
    },
    Segmented: {
      itemSelectedBg: '#38618c',
      itemSelectedColor: '#fff',
      trackBg: 'rgba(56, 97, 140, 0.06)',
    },
  },
};

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ConfigProvider locale={zhCN} theme={customTheme}>
        <CoverageMatrixProvider>
          <AppShell />
        </CoverageMatrixProvider>
      </ConfigProvider>
    </QueryClientProvider>
  );
}

export default App;
