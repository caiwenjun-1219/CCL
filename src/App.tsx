import { Routes, Route, Navigate } from "react-router-dom";
import Layout from "./components/Layout";
import HomePage from "./pages/HomePage";
import InstancesPage from "./pages/InstancesPage";
import VersionsPage from "./pages/VersionsPage";
import ModsPage from "./pages/ModsPage";
import ModManagePage from "./pages/ModManagePage";
import InstanceDetailPage from "./pages/InstanceDetailPage";
import ResourcePage from "./pages/ResourcePage";
import ModpacksPage from "./pages/ModpacksPage";
import JavaPage from "./pages/JavaPage";
import AccountsPage from "./pages/AccountsPage";
import SettingsPage from "./pages/SettingsPage";
import LinkPage from "./pages/LinkPage";
import AboutPage from "./pages/AboutPage";

/**
 * PCL 式五页签结构：启动 / 下载 / 联机 / 设置 / 更多
 * 下载、设置、更多 各带 152px 子导航（由 Layout 依路由渲染）
 */
export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        {/* 启动 */}
        <Route path="/" element={<HomePage />} />

        {/* 下载 */}
        <Route path="/download" element={<Navigate to="/download/game" replace />} />
        <Route path="/download/game" element={<VersionsPage />} />
        <Route path="/download/mod" element={<ModsPage />} />
        <Route path="/download/pack" element={<ModpacksPage />} />
        <Route path="/download/resourcepack" element={<ResourcePage kind="resourcepack" />} />
        <Route path="/download/shader" element={<ResourcePage kind="shader" />} />
        <Route path="/download/java" element={<JavaPage />} />

        {/* 联机 */}
        <Route path="/link" element={<LinkPage />} />

        {/* 设置 */}
        <Route path="/settings" element={<SettingsPage />} />

        {/* 更多 */}
        <Route path="/more" element={<Navigate to="/more/version" replace />} />
        <Route path="/more/version" element={<InstancesPage />} />
        {/* 实例详情（PCL 的「版本设置」） */}
        <Route path="/instance/:id" element={<InstanceDetailPage />} />
        <Route path="/more/mods" element={<ModManagePage />} />
        <Route path="/more/account" element={<AccountsPage />} />
        <Route path="/more/about" element={<AboutPage />} />

        {/* 旧路由兼容 */}
        <Route path="/versions" element={<Navigate to="/download/game" replace />} />
        <Route path="/mods" element={<Navigate to="/download/mod" replace />} />
        <Route path="/mod-manage" element={<Navigate to="/more/mods" replace />} />
        <Route path="/modpacks" element={<Navigate to="/download/pack" replace />} />
        <Route path="/java" element={<Navigate to="/download/java" replace />} />
        <Route path="/instances" element={<Navigate to="/more/version" replace />} />
        <Route path="/accounts" element={<Navigate to="/more/account" replace />} />

        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
