import { createApp } from "vue";
import App from "./App.vue";
import "./style.css";

// 清理历史版本 NGlobalStyle 遗留在 body 上的内联样式
// （该组件卸载时只移除 n-styled 标记，color/transition 等内联样式会残留）
if (document.body.hasAttribute("n-styled")) {
  document.body.removeAttribute("n-styled");
  document.body.style.cssText = "";
}

createApp(App).mount("#app");
