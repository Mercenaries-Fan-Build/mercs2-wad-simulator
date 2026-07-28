LocalWidgetList = {}
if import then
  import("mrxguishell")
else
  mrxguishell = {}
end
LocalWidgetList[1] = {
  name = "Shell",
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
  EventHandlerFile = "mrxguishell",
  EventHandlers = {
    LobbyServerUpdated = mrxguishell.HandleServerUpdate,
    LobbyServerAdded = mrxguishell.HandleServerAdd,
    ControllerInput = mrxguishell.HandleInput,
    LobbyServerRemoved = mrxguishell.HandleServerRemove,
    GuiInitialization = mrxguishell.HandleInitializationEvent,
    GuiGameStateChange = mrxguishell.HandleGameStateChangeEvent
  },
  EventHandlerNames = {
    LobbyServerUpdated = "HandleServerUpdate",
    LobbyServerAdded = "HandleServerAdd",
    ControllerInput = "HandleInput",
    LobbyServerRemoved = "HandleServerRemove",
    GuiInitialization = "HandleInitializationEvent",
    GuiGameStateChange = "HandleGameStateChangeEvent"
  },
  Children = {
    {
      name = "Shell Background",
      x1 = 0,
      y1 = 0,
      x2 = 640,
      y2 = 480,
      RedLevel = 0,
      GreenLevel = 0,
      BlueLevel = 0,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "center",
      visible = 1,
      container = false,
      WidgetType = "image",
      texture = nil,
      textureFile = nil,
      rotation = 0,
      u1 = 0,
      v1 = 0,
      u2 = 1,
      v2 = 1,
      EventHandlerFile = "mrxguishell",
      EventHandlers = {
        GuiInitialization = mrxguishell.MakeFullscreen
      },
      EventHandlerNames = {
        GuiInitialization = "MakeFullscreen"
      },
      Children = {
        {
          name = "New Text",
          x1 = 120,
          y1 = 390,
          x2 = 520,
          y2 = 406,
          RedLevel = 255,
          GreenLevel = 255,
          BlueLevel = 255,
          TranslucencyLevel = 255,
          HorizontalAnchor = "center",
          VerticalAnchor = "bottom",
          visible = 1,
          container = false,
          WidgetType = "text",
          text = "",
          font = "font_16",
          scale = 1,
          Justification = "center",
          EventHandlerFile = "",
          EventHandlers = {},
          EventHandlerNames = {},
          Children = {}
        }
      }
    }
  }
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
