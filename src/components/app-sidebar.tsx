import { NavLink, useLocation } from "react-router";
import {
  BanknoteArrowUpIcon,
  CreditCardPlusIcon,
  CreditCardReaderIcon,
  HouseIcon,
} from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from "@/components/ui/sidebar";

const navItems = [
  { title: "Home", url: "/", icon: HouseIcon },
  { title: "Upload de fatura", url: "/upload-fatura", icon: CreditCardPlusIcon },
  {
    title: "Gastos cartão de crédito",
    url: "/gastos-cartao",
    icon: CreditCardReaderIcon,
  },
  { title: "Entradas", url: "/entradas", icon: BanknoteArrowUpIcon },
];

export function AppSidebar() {
  const { pathname } = useLocation();

  return (
    <Sidebar collapsible="icon">
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              {navItems.map((item) => (
                <SidebarMenuItem key={item.url}>
                  <SidebarMenuButton
                    asChild
                    isActive={
                      item.url === "/"
                        ? pathname === "/"
                        : pathname.startsWith(item.url)
                    }
                    tooltip={item.title}
                  >
                    <NavLink to={item.url} end={item.url === "/"}>
                      <item.icon />
                      <span>{item.title}</span>
                    </NavLink>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarRail />
    </Sidebar>
  );
}
