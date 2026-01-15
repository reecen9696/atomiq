import { NavLink, Route, Routes } from "react-router-dom";
import "./App.css";
import { CoinflipPage } from "./pages/CoinflipPage";
import { VerifyPage } from "./pages/VerifyPage";

export default function App() {
  return (
    <div className="app">
      <header className="appHeader">
        <div className="brand">
          <div className="brandTitle">Atomik Network</div>
          <div className="brandSub">Finalized bets + browser verification</div>
        </div>
        <nav className="nav">
          <NavLink to="/" end>
            Bet
          </NavLink>
          <NavLink to="/verify">Verify</NavLink>
        </nav>
      </header>

      <main className="appMain">
        <Routes>
          <Route path="/" element={<CoinflipPage />} />
          <Route path="/verify" element={<VerifyPage />} />
        </Routes>
      </main>
    </div>
  );
}
