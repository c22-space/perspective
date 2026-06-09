import { NavLink, Outlet } from 'react-router-dom';

const nav = [
  { to: '/', label: 'Overview' },
  { to: '/processes', label: 'Processes' },
  { to: '/graph', label: 'Graph' },
  { to: '/memories', label: 'Memories' },
  { to: '/config', label: 'Config' },
  { to: '/logs', label: 'Logs' },
];

export default function Layout() {
  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <aside className="w-56 shrink-0 border-r border-zinc-800 flex flex-col">
        <div className="px-4 py-5 border-b border-zinc-800">
          <h1 className="text-lg font-bold tracking-tight">
            <span className="bg-gradient-to-r from-blue-400 to-emerald-400 bg-clip-text text-transparent">
              perspective
            </span>
          </h1>
          <p className="text-xs text-zinc-500 mt-0.5">memory engine</p>
        </div>
        <nav className="flex-1 py-3">
          {nav.map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `block px-4 py-2 text-sm transition-colors ${
                  isActive
                    ? 'bg-zinc-800/60 text-white border-r-2 border-blue-400'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/30'
                }`
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="px-4 py-3 border-t border-zinc-800 text-xs text-zinc-600">
          v0.1.0
        </div>
      </aside>
      <main className="flex-1 overflow-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}
