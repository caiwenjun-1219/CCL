import { Routes, Route, Navigate } from "react-router-dom";
import Layout from "./components/Layout";
import HomePage from "./pages/HomePage";
import InstancesPage from "./pages/InstancesPage";
import VersionsPage from "./pages/VersionsPage";
import ModsPage from "./pages/ModsPage";
import ModpacksPage from "./pages/ModpacksPage";
import JavaPage from "./pages/JavaPage";
import AccountsPage from "./pages/AccountsPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<HomePage />} />
        <Route path="/instances" element={<InstancesPage />} />
        <Route path="/versions" element={<VersionsPage />} />
        <Route path="/mods" element={<ModsPage />} />
        <Route path="/modpacks" element={<ModpacksPage />} />
        <Route path="/java" element={<JavaPage />} />
        <Route path="/accounts" element={<AccountsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
