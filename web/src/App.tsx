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
 * Custom Ant Design theme — 「研究手稿」视觉系统。
 *
 * 与 index.css 的 CSS 变量同源：学术墨蓝主色、暖纸底色、朱砂点缀。
 * 标题走思源宋体（serif-display 工具类），正文保持无衬线以保证可读性。
 */
const customTheme = {
  algorithm: theme.defaultAlgorithm,
  token: {
    colorPrimary: '#244f73', // 学术墨蓝
    colorInfo: '#244f73',
    colorSuccess: '#27715d',
    colorWarning: '#9a6417',
    colorError: '#c33b2d', // 朱砂红（与批注色一致）
    colorTextBase: '#27394a',
    colorTextSecondary: '#687889',
    borderRadius: 7,
    borderRadiusLG: 10,
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", "PingFang SC", "Microsoft YaHei", "Source Han Sans CN", sans-serif',
    fontSize: 14,
    colorBgLayout: '#f4f0e7', // 暖纸底色
    colorBgContainer: '#fffefa',
    colorBorderSecondary: '#ded9cd',
    boxShadowTertiary:
      '0 10px 30px -24px rgba(24, 48, 72, 0.42), 0 1px 0 rgba(24, 48, 72, 0.05)',
  },
  components: {
    Layout: {
      headerBg: '#faf7f0',
      bodyBg: 'transparent', // 允许透出底层的 CSS 渐变色
      siderBg: '#f7f3ea',
    },
    Card: {
      borderRadiusLG: 8,
      colorBorderSecondary: '#ded9cd',
      headerBg: 'transparent',
    },
    Button: {
      controlHeight: 38,
      borderRadius: 7,
    },
    Segmented: {
      itemSelectedBg: '#244f73',
      itemSelectedColor: '#fff',
      trackBg: '#ece8df',
    },
    Drawer: {
      colorBgElevated: '#fffefa',
    },
    Input: {
      activeBorderColor: '#52799a',
      hoverBorderColor: '#7592aa',
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
