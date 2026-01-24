import { NavLink, useLocation } from 'react-router-dom'

const navItems = [
  { path: '/home', icon: 'home', label: 'Home' },
  { path: '/discover', icon: 'explore', label: 'Discover' },
  { path: '/sessions', icon: 'conversion_path', label: 'Sessions' },
  { path: '/parsers', icon: 'code', label: 'Parsers' },
  { path: '/jobs', icon: 'play_circle', label: 'Jobs' },
  { path: '/approvals', icon: 'verified_user', label: 'Approvals' },
  { path: '/query', icon: 'terminal', label: 'Query' },
]

export default function Sidebar() {
  const location = useLocation()

  return (
    <aside className="sidebar" data-testid="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-logo">CASPARIAN</span>
      </div>

      <div className="sidebar-content">
        <div className="sidebar-section-title">Navigation</div>
        {navItems.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            className={`sidebar-nav-item ${location.pathname === item.path || location.pathname.startsWith(item.path + '/') ? 'active' : ''}`}
            data-testid={`nav-${item.label.toLowerCase()}`}
          >
            <span className="material-symbols-sharp">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </div>

      <div className="sidebar-footer">
        <NavLink
          to="/settings"
          className={`sidebar-nav-item ${location.pathname === '/settings' ? 'active' : ''}`}
          data-testid="nav-settings"
        >
          <span className="material-symbols-sharp">settings</span>
          Settings
        </NavLink>
      </div>
    </aside>
  )
}
