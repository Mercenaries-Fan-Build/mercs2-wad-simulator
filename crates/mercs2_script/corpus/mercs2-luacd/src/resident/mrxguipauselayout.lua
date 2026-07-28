LocalWidgetList = {}
if import then
  import("MrxGuiPauseScreen")
else
  MrxGuiPauseScreen = {}
end
LocalWidgetList[1] = {
  name = "Pause Layout",
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
  EventHandlerFile = "MrxGuiPauseScreen",
  EventHandlers = {
    GuiGameStateChange = MrxGuiPauseScreen.HandleStateChangeEvent,
    GuiInitialization = MrxGuiPauseScreen._Initialize,
    TogglePAUSE = MrxGuiPauseScreen._HandleToggleEvent,
    ImposterShellEvent = MrxGuiPauseScreen.HandleImposterEvent
  },
  EventHandlerNames = {
    GuiGameStateChange = "HandleStateChangeEvent",
    GuiInitialization = "_Initialize",
    TogglePAUSE = "_HandleToggleEvent",
    ImposterShellEvent = "HandleImposterEvent"
  },
  Children = {}
}
if import then
  import("MrxGuiBase")
end

function ReInit()
  for i in pairs(AddedWidgetList) do
    MrxGuiBase.RemoveWidget(AddedWidgetList[i])
  end
  AddedWidgetList = {}
  for i in pairs(LocalWidgetList) do
    MrxGuiBase.LoadAndAddWidgetFromLayoutFileData(LocalWidgetList[i], AddedWidgetList)
  end
end
