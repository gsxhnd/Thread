import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import zhCNCommon from "./locales/zh-CN/common.json";
import enUSCommon from "./locales/en-US/common.json";

const resources = {
  "zh-CN": {
    common: zhCNCommon,
  },
  "en-US": {
    common: enUSCommon,
  },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: "zh-CN",
  fallbackLng: "en-US",
  defaultNS: "common",
  ns: ["common"],
  interpolation: {
    escapeValue: false,
  },
});

export default i18n;
