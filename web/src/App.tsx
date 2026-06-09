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
 * Custom Ant Design theme.
 *
 * Inspired by Lobe Chat / ChatGPT modern UI: subtle blue primary, generous
 * border radii, soft shadow tokens. Falls back to system color scheme via
 * `prefers-color-scheme: dark` automatically.
 */
const customTheme = {
  algorithm: theme.defaultAlgorithm,
  token: {
    colorPrimary: '#38618c', // 莫兰迪学术蓝
    borderRadius: 10,
    borderRadiusLG: 14,
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", "PingFang SC", "Microsoft YaHei", "Source Han Sans CN", sans-serif',
    fontSize: 14,
    colorBgLayout: '#f4f2ea', // 暖米黄底色
  },
  components: {
    Layout: {
      headerBg: 'rgba(250, 249, 245, 0.85)',
      bodyBg: 'transparent', // 允许透出底层的 CSS 渐变色
      siderBg: 'rgba(250, 249, 245, 0.85)',
    },
    Card: {
      borderRadiusLG: 14,
    },
    Button: {
      controlHeight: 36,
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
