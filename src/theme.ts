import type { GlobalThemeOverrides } from "naive-ui";

// Naive UI derives component colors through seemly/rgba at runtime, so these
// bridge values must stay concrete; CSS-native surfaces use the tokens in style.css.
export const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#007aff",
    primaryColorHover: "#0071e3",
    primaryColorPressed: "#006edb",
    primaryColorSuppl: "#007aff",
    successColor: "#34c759",
    warningColor: "#ff9f0a",
    errorColor: "#ff3b30",
    errorColorHover: "#d92c24",
    errorColorPressed: "#c4251d",
    errorColorSuppl: "#ff3b30",
    infoColor: "#007aff",
    borderRadius: "10px",
    borderRadiusSmall: "8px",
    fontSizeMedium: "14px",
  },
  Card: {
    borderRadius: "16px",
  },
  Dialog: {
    borderRadius: "16px",
  },
  Modal: {
    borderRadius: "16px",
  },
  Input: {
    colorFocus: "#ffffff",
  },
  Button: {
    borderRadius: "8px",
    fontWeight: "600",
  },
};

/** 深色主题覆盖：跟随 linguagram 深色配色（accent 抬升、表面 #24242a） */
export const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#5e9cff",
    primaryColorHover: "#79adff",
    primaryColorPressed: "#1f6feb",
    primaryColorSuppl: "#5e9cff",
    successColor: "#3fb950",
    warningColor: "#ff9f0a",
    errorColor: "#ff3b30",
    errorColorHover: "#d92c24",
    errorColorPressed: "#c4251d",
    errorColorSuppl: "#ff3b30",
    infoColor: "#5e9cff",
    bodyColor: "#1a1d1f",
    borderRadius: "10px",
    borderRadiusSmall: "8px",
    fontSizeMedium: "14px",
  },
  Card: {
    borderRadius: "16px",
    color: "#24242a",
  },
  Dialog: {
    color: "#24242a",
    borderRadius: "16px",
  },
  Modal: {
    color: "#24242a",
    borderRadius: "16px",
  },
  Popover: {
    color: "#24242a",
  },
  Input: {
    color: "#24242a",
    colorFocus: "#24242a",
    border: "1px solid rgba(255, 255, 255, 0.24)",
    borderHover: "1px solid rgba(255, 255, 255, 0.42)",
    borderFocus: "1px solid #5e9cff",
  },
  Button: {
    borderRadius: "8px",
    fontWeight: "600",
  },
};
