import { NavLink, useLocation } from "react-router-dom";
import type { LucideIcon } from "lucide-react";

export interface SubNavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  badge?: string | number;
}

export interface SubNavGroup {
  title?: string;
  items: SubNavItem[];
}

/** PCL 左侧子导航栏：152px 宽，36px 行高，选中浅蓝底 + 主题色文字 */
export default function SubNav({
  groups,
  footer,
}: {
  groups: SubNavGroup[];
  footer?: React.ReactNode;
}) {
  const location = useLocation();
  return (
    <>
      <div className="flex-1 overflow-y-auto py-2">
        {groups.map((g, gi) => (
          <div key={gi} className={gi > 0 ? "mt-1" : ""}>
            {g.title && (
              <div className="px-4 pt-3 pb-1 text-[11px] font-bold text-ccl-gray4 tracking-wide">
                {g.title}
              </div>
            )}
            {g.items.map((it) => {
              const active = location.pathname + location.search === it.to;
              const Icon = it.icon;
              return (
                <NavLink
                  key={it.to}
                  to={it.to}
                  className="pcl-nav-item"
                  data-active={active}
                >
                  <Icon size={14} strokeWidth={2} />
                  <span className="flex-1 truncate">{it.label}</span>
                  {it.badge !== undefined && it.badge !== "" && (
                    <span className="tag tag-blue">{it.badge}</span>
                  )}
                </NavLink>
              );
            })}
          </div>
        ))}
      </div>
      {footer && (
        <div className="px-4 py-2.5 text-[11px] text-ccl-gray4 leading-relaxed shrink-0">
          {footer}
        </div>
      )}
    </>
  );
}
