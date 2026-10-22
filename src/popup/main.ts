import { mount } from "svelte";
import "../lib/theme.css";
import Popup from "./Popup.svelte";

// 开发模式下 `popup.html?mock` 使用模拟后端，可在普通浏览器中调试界面
if (import.meta.env.DEV && new URLSearchParams(location.search).has("mock")) {
  const { installMock } = await import("../lib/devMock");
  installMock("popup");
}

mount(Popup, { target: document.getElementById("app")! });
