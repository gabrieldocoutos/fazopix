import { HashRouter, Route, Routes } from "react-router";
import { AppLayout } from "@/components/app-layout";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import Home from "@/pages/home";
import UploadFatura from "@/pages/upload-fatura";

function App() {
  return (
    <TooltipProvider>
      <HashRouter>
        <Routes>
          <Route element={<AppLayout />}>
            <Route index element={<Home />} />
            <Route path="upload-fatura" element={<UploadFatura />} />
          </Route>
        </Routes>
      </HashRouter>
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}

export default App;
