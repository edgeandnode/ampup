import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router";
import "./globals.css";
import Providers from "@/components/Providers";
import Page from "@/components/Page";

function App() {
  return (
    <Providers>
      <main id="main">
        <Routes>
          <Route path="/" element={<Page />} />
        </Routes>
      </main>
    </Providers>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </StrictMode>,
);
