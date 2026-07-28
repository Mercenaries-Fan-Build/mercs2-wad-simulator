LocalWidgetList = {}
if import then
  import("MrxGuiLTIPrecache")
else
  MrxGuiLTIPrecache = {}
end
LocalWidgetList[1] = {
  name = "LTI_precache",
  x1 = 0,
  y1 = 0,
  x2 = 640,
  y2 = 480,
  RedLevel = 255,
  GreenLevel = 255,
  BlueLevel = 255,
  TranslucencyLevel = 255,
  HorizontalAnchor = "center",
  VerticalAnchor = "center",
  visible = 1,
  container = false,
  EventHandlerFile = "MrxGuiLTIPrecache",
  EventHandlers = {
    GuiGameStateChange = MrxGuiLTIPrecache.HandleStateChangeEvent,
    GuiInitialization = MrxGuiLTIPrecache._Initialize
  },
  EventHandlerNames = {
    GuiGameStateChange = "HandleStateChangeEvent",
    GuiInitialization = "_Initialize"
  },
  Children = {}
}
