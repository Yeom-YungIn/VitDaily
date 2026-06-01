export default function App() {
  return (
    <div className="flex flex-col h-screen bg-slate-900 text-slate-100">
      <header className="flex items-center justify-between px-5 py-3 border-b border-slate-700 bg-slate-900">
        <div className="flex items-center gap-2">
          <span className="text-orange-400 text-xl font-bold">₿</span>
          <span className="font-semibold text-slate-100 tracking-tight">VitDaily</span>
        </div>
      </header>

      <main className="flex-1 overflow-auto p-5">
        <section className="flex h-full flex-col items-center justify-center gap-3 text-center">
          <h1 className="text-xl font-semibold text-white">VitDaily</h1>
          <p className="max-w-xs text-sm text-slate-400">
            매일 비트코인 자동 매수를 설정하는 데스크탑 앱입니다.
          </p>
        </section>
      </main>
    </div>
  );
}
