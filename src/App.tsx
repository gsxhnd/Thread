import { RouterProvider } from "@tanstack/react-router";
import { AppProviders } from "@/layout/providers/app-providers";
import { router } from "@/app/router";

function App() {
  return (
    <AppProviders>
      <RouterProvider router={router} />
    </AppProviders>
  );
}

export default App;
