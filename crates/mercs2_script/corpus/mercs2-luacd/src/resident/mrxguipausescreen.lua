import("MrxGuiBase")
import("MrxGuiManager")
import("MrxGuiDialogBox")
import("MrxSound")
import("MrxPlayState")
import("WifMissionFlow")
import("MrxPlayer")
import("MrxTutorialManager")
import("MrxStatsManager")
import("MrxUtil")
Joystick = {
  BUTTON_PAD1_U = 1,
  BUTTON_PAD1_D = 2,
  BUTTON_PAD1_L = 3,
  BUTTON_PAD1_R = 4,
  BUTTON_PAD2_U = 5,
  BUTTON_PAD2_D = 6,
  BUTTON_PAD2_L = 7,
  BUTTON_PAD2_R = 8,
  BUTTON_L_STICK_L = 9,
  BUTTON_L_STICK_R = 10,
  BUTTON_L_STICK_U = 11,
  BUTTON_L_STICK_D = 12,
  BUTTON_R_STICK_L = 13,
  BUTTON_R_STICK_R = 14,
  BUTTON_R_STICK_U = 15,
  BUTTON_R_STICK_D = 16,
  BUTTON_ALT1_1 = 17,
  BUTTON_ALT1_2 = 18,
  BUTTON_ALT1_3 = 19,
  BUTTON_ALT2_1 = 20,
  BUTTON_ALT2_2 = 21,
  BUTTON_ALT2_3 = 22,
  BUTTON_SYS1 = 23,
  BUTTON_SYS2 = 24
}
tControlMap = false

function Init()
  tControlMap = {
    human = {
      [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Support_Menu]",
      [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Support_Menu]",
      [Joystick.BUTTON_PAD1_L] = "[SHELL.Controls.Switch_Explosive]",
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Action]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Jump]",
      [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reload]",
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Melee]",
      [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.LeftRight]",
      [Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.UpDown]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.TurnLR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]",
      [Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Crouch]",
      [Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.Use_Explosive]",
      [Joystick.BUTTON_ALT1_3] = "[SHELL.Controls.Sprint]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Zoom]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    },
    car = {
      [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]",
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]",
      [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]",
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]",
      [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]",
      [Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    },
    tank = {
      [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]",
      [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]",
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]",
      [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]",
      [Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Move]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Aim_LR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    },
    helicopter = {
      [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_L] = "[SHELL.Controls.Winch]",
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Raise]",
      [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Lower]",
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]",
      [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.LeftRight]",
      [Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Move]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.TurnLR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]",
      [Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Lower]",
      [Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.Raise]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    },
    jet = {
      [Joystick.BUTTON_PAD1_U] = "Seat Menu",
      [Joystick.BUTTON_PAD1_D] = "Seat Menu",
      [Joystick.BUTTON_PAD2_U] = "Exit",
      [Joystick.BUTTON_PAD2_D] = "Accelerate",
      [Joystick.BUTTON_PAD2_L] = "Decelerate",
      [Joystick.BUTTON_PAD2_R] = "Winch",
      [Joystick.BUTTON_L_STICK_L] = "Roll L/R",
      [Joystick.BUTTON_L_STICK_U] = "Pitch U/D",
      [Joystick.BUTTON_R_STICK_L] = "Turn L/R",
      [Joystick.BUTTON_R_STICK_U] = "Aim U/D",
      [Joystick.BUTTON_ALT1_1] = "Lower",
      [Joystick.BUTTON_ALT1_2] = "Raise",
      [Joystick.BUTTON_ALT1_3] = "Toggle VTOL",
      [Joystick.BUTTON_ALT2_1] = "Fire Weapon",
      [Joystick.BUTTON_ALT2_2] = "Switch Weapon",
      [Joystick.BUTTON_ALT2_3] = "Camera Mode",
      [Joystick.BUTTON_SYS1] = "Pause",
      [Joystick.BUTTON_SYS2] = "PDA"
    },
    boat = {
      [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]",
      [Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]",
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]",
      [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]",
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]",
      [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]",
      [Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    },
    ladder = {
      [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Exit]",
      [Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Climb_UD]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Look_LR]"
    },
    seat = {
      [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]",
      [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Aim_LR]",
      [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]",
      [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
      [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
      [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Camera_Mode]",
      [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
      [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
    }
  }
end

function OpenPauseScreen(oPauseMenu)
  if MrxGuiDialogBox.oSystemDialogBoxFlash then
    return
  end
  if oPauseMenu.CustomData.bActive then
    return
  end
  if not oPauseMenu.CustomData.bHaveFlash and not oPauseMenu.CustomData.bLoading then
    local oFlash = oPauseMenu.CustomData.oMapFlash
    MrxGuiBase.AddWidget(oFlash)
    oPauseMenu.CustomData.bLoading = true
    oFlash:SetSwfFile(oFlash.CustomData.sFile, _FinishPauseOpen, {oPauseMenu})
  end
end

function _FinishPauseOpen(oPauseMenu)
  _FinishLoad(oPauseMenu)
  MrxSound.EnterPauseState()
  oPauseMenu.CustomData.oMapFlash:Restart()
  oPauseMenu.CustomData.oMapFlash:Play()
  oPauseMenu.CustomData.bActive = true
  MrxGuiBase.GetControlFocus(oPauseMenu, true)
  oPauseMenu:SetVisible(true)
  local tChildren = oPauseMenu:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.AddWidgetWithChildren(oChild)
  end
  local oFlash = oPauseMenu.CustomData.oMapFlash
  local inMission = MrxPlayState.Get() == MrxPlayState._knMission
  local lastMission = WifMissionFlow.GetLastCompletedContractName() or "none"
  local fMissionTime = MrxPlayState.GetTotalTimeElapsed()
  _GuiInternal.SetFlashPauseMenu(oFlash.BasicData.uId, inMission, lastMission, fMissionTime, MrxStatsManager.GetPercentCompleted())
  oFlash:SetFlashEventHandler("LTIFscommand", _LTIFscommand, {})
  oFlash:SetFlashEventHandler("LTIVideoSetGamma", _LTIVideoSetGamma, {})
  oFlash:SetFlashEventHandler("LTIVideoSwitchOpt1", _LTIVideoSwitchOpt1, {})
  oFlash:SetFlashEventHandler("LTIInputGeneralRumble", _LTIInputGeneralRumble, {})
  oFlash:SetFlashEventHandler("LTIInputKMChangeInput", _LTIInputKMChangeInput, {})
  oFlash:SetFlashEventHandler("LTIOverBoundResponse", _LTIOverBoundResponse, {})
  oFlash:SetFlashEventHandler("LTIInputJoystickChangePrimary", _LTIInputJoystickChangePrimary, {})
  oFlash:SetFlashEventHandler("LTIInputJoystickChangeInput", _LTIInputJoystickChangeInput, {})
  oFlash:SetFlashEventHandler("LTIJoystickOverBoundResponse", _LTIJoystickOverBoundResponse, {})
  oFlash:SetFlashEventHandler("PauseItemChanged", _LTIPauseItemChanged, {})
  oFlash:SetFlashEventHandler("LTICamera", _LTICamera, {})
  local uPlayerGuid = Player.GetLocalPlayer()
  if uPlayerGuid then
    local bEnabled = MrxGuiManager.GetHudState(uPlayerGuid)
    oPauseMenu.CustomData.tHudStates[uPlayerGuid] = bEnabled
    if bEnabled then
      MrxGuiManager.ToggleHud(uPlayerGuid, false)
    end
    local sControlType = Player.GetControlBindingType(uPlayerGuid)
    local tControl = tControlMap[sControlType] or {}
    for nControlIndex, sControlLabel in pairs(tControl) do
      oFlash:CallActionScriptCallback("controllerDisplay", {nControlIndex, sControlLabel})
    end
  end
  local bSubtitles = true
  if Sys.SubtitlesEnabled then
    bSubtitles = Sys.SubtitlesEnabled()
  end
  oFlash:CallActionScriptCallback("videoSubtitles", {bSubtitles})
  local bRumble = true
  if Sys.RumbleEnabled then
    bRumble = Sys.RumbleEnabled()
  end
  oFlash:CallActionScriptCallback("gameRumble", {bRumble})
  bTutorials = true
  if Sys.TutorialsEnabled then
    bTutorials = Sys.TutorialsEnabled()
  end
  oFlash:CallActionScriptCallback("gameTutorials", {bTutorials})
  local bInvert = false
  if Sys.YAxisInverted then
    bInvert = Sys.YAxisInverted()
  end
  oFlash:CallActionScriptCallback("gameinvert", {bInvert})
  local bMissionState = false
  if not MrxPlayState.IsFree() then
    bMissionState = true
  end
  oFlash:CallActionScriptCallback("activeContract", {bMissionState})
  local bCanMedEvac = MrxPlayer.CanMedEvac()
  oFlash:CallActionScriptCallback("medevacEnable", {bCanMedEvac})
  if oPauseMenu.CustomData.bSaveDisabled then
    oFlash:CallActionScriptCallback("disableSave", {true})
  end
end

function ClosePauseScreen(oPauseMenu)
  if not oPauseMenu.CustomData.bActive then
    return
  end
  oPauseMenu.CustomData.bActive = false
  MrxGuiBase.ReleaseControlFocus(oPauseMenu)
  if oPauseMenu.CustomData.bHaveFlash then
    oPauseMenu.CustomData.oMapFlash:CallActionScriptCallback("saveProfile", {})
    oPauseMenu.CustomData.oMapFlash:Pause()
    oPauseMenu.CustomData.oMapFlash:SetSwfFile(nil)
    oPauseMenu.CustomData.bHaveFlash = false
  elseif oPauseMenu.CustomData.bLoading then
    oPauseMenu.CustomData.oMapFlash:SetSwfFile(nil)
    oPauseMenu.CustomData.bLoading = false
  end
  oPauseMenu:SetVisible(false)
  local tChildren = oPauseMenu:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidgetWithChildren(oChild)
  end
  if _GuiInternal.RemoveFlashPauseMenu then
    _GuiInternal.RemoveFlashPauseMenu(oPauseMenu.CustomData.oMapFlash.BasicData.uId)
  end
  local uPlayerGuid = Player.GetLocalPlayer()
  if uPlayerGuid and oPauseMenu.CustomData.tHudStates[uPlayerGuid] then
    MrxGuiManager.ToggleHud(uPlayerGuid, true)
    oPauseMenu.CustomData.tHudStates[uPlayerGuid] = nil
  end
  MrxSound.ExitPauseState()
  local nTime = 3
  local tEvent = {}
  tEvent.EventType = "GuiShowAmmoCounter"
  tEvent.bShowGun = true
  tEvent.bShowExplosive = true
  tEvent.nTime = nTime
  MrxGuiBase.SentEvent(tEvent)
  local tHealthEvent = {}
  tHealthEvent.EventType = "ShowAllCounters"
  tHealthEvent.nTime = nTime
  MrxGuiBase.SentEvent(tHealthEvent)
  if bTutorials == true and Sys.TutorialsEnabled() == false then
    MrxTutorialManager.HideMessage(true)
  end
end

function SetUserSaveEnabled(oPause, bEnable)
  if nil ~= bEnable then
    oPause.CustomData.bSaveDisabled = not bEnable
  end
end

function HandleStateChangeEvent(oWidget, sStateName, sStateAction)
  if oWidget.CustomData.bImposterEnabled then
    return
  end
  if not sStateName or not sStateAction then
    return
  end
  if "Pause" ~= sStateName then
    return
  end
  if "Enter" == sStateAction then
    OpenPauseScreen(oWidget)
  elseif "Exit" == sStateAction then
    ClosePauseScreen(oWidget)
  end
  MrxGuiBase.ChangeScreenResolution()
end

function HandleInitializationEvent(oWidget, tUnused)
  oWidget:SetUseImmortalEvents(true)
  oWidget.CustomData.oMenu = oWidget:GetChildren()[3]
  oWidget:GetChildren()[1]:SetFullscreen(true)
  ClosePauseScreen(oWidget)
  oWidget:SetEventHandler("GuiStateChangeEvent", nil)
  oWidget:SetEventHandler("GuiStateChangeEvent", HandleStateChangeEvent)
  oWidget.CustomData.tHudStates = {}
end

function _Initialize(oPauseMenu)
  oPauseMenu:SetUseImmortalEvents(true)
  oPauseMenu.CustomData.bActive = true
  local oBg = MrxGuiBase.ImageWidget:new()
  oBg:SetTransient(false)
  oBg:SetFullscreen(true)
  oBg:SetColor(0, 0, 0, 192)
  oPauseMenu:AddChild(oBg)
  oBg.oParentWidget = oPauseMenu
  local oMapFlash = MrxGuiBase.FlashWidget:new()
  oMapFlash:SetTransient(false)
  oMapFlash:SetAnchoring("center", "center")
  oMapFlash:SetFullscreen(true)
  oPauseMenu:AddChild(oMapFlash)
  oPauseMenu.CustomData.oMapFlash = oMapFlash
  oMapFlash.CustomData.sFile = "pause_menu"
  oMapFlash.oParentWidget = oPauseMenu
  oPauseMenu.Open = OpenPauseScreen
  oPauseMenu.Close = ClosePauseScreen
  oPauseMenu.SetUserSaveEnabled = SetUserSaveEnabled
  oPauseMenu:SetEventHandler("ControllerInput", _HandleInput)
  oPauseMenu.nAnalogInputHeld = 0
  oPauseMenu.CustomData.tHudStates = {}
  Pg.LoadAsset("pause_graphic", "texture")
  oPauseMenu:Close()
end

function _FinishLoad(oPauseMenu)
  oPauseMenu.CustomData.bHaveFlash = true
  oPauseMenu.CustomData.bLoading = false
  local oFlash = oPauseMenu.CustomData.oMapFlash
  oFlash:SetFlashEventHandler("quitGame", _HandleQuitEvent)
  oFlash:SetFlashEventHandler("closePause", _HandleCloseEvent)
  oFlash:SetFlashEventHandler("messageMedEvac", _ConfirmMedEvacEvent)
  oFlash:SetFlashEventHandler("messageButton", _HandleMedEvacEvent, {})
  oFlash:Pause()
end

function _HandleToggleEvent(oPauseMenu, tUnused)
  if oPauseMenu.CustomData.bActive then
    oPauseMenu:Close()
  else
    oPauseMenu:Open()
  end
end

function _HandleInput(oPauseMenu, tInput)
  local oMap = oPauseMenu.CustomData.oMapFlash
  oMap.EventHandlers.ControllerInput(oMap, tInput)
end

function _HandleCloseEvent(oMapFlash)
  local oPauseMenu = oMapFlash.oParentWidget
  if oPauseMenu.CustomData.bActive then
    Sys.RequestGameState("ingame")
    oPauseMenu:Close()
  end
end

function _HandleQuitEvent(oMapFlash)
  local oPauseMenu = oMapFlash.oParentWidget
  Sys.RequestGameState("unloading")
  Net.QuitGame()
  oPauseMenu:Close()
end

function _ConfirmMedEvacEvent(oMapFlash)
  local sMoney = MrxUtil.FormatMoney(MrxPlayer.GetMedEvacCost())
  oMapFlash:CallActionScriptCallback("onlineMessage", {
    "[SHELL.Confirmation.AreYouSure]",
    "[PauseMenu.Base.MedEvacMessage] " .. sMoney .. " [PauseMenu.Base.MedEvacMessageEnd]",
    2,
    "[SHELL.Confirmation.Yes]",
    "[SHELL.Confirmation.No]",
    "messageMedEvac",
    true
  })
  _bMedEvac = true
end

function _HandleMedEvacEvent(oMapFlash, sButton)
  if not _bMedEvac then
    return
  end
  _bMedEvac = nil
  oMapFlash:CallActionScriptCallback("onlineMessageClose", {})
  if sButton == "1" then
    MrxPlayer.MedEvac()
    Sys.RequestGameState("ingame")
  end
end

function HandleImposterInitializationEvent(oWidget, tEvent)
  oWidget:GetChildren()[1]:SetFullscreen(true)
  oWidget:SetVisible(false)
  oWidget.CustomData.bReceiveInput = false
  for nIndex, oChild in pairs(oWidget:GetChildren()) do
    oChild:SetEnabled(false)
  end
end

function HandleImposterStateChangeEvent(oWidget, sStateName, sStateAction)
  if not oWidget.CustomData.bImposterEnabled then
    return
  end
  if not sStateName or not sStateAction then
    return
  end
  if "Pause" ~= sStateName then
    return
  end
  if "Enter" == sStateAction then
    oWidget:SetVisible(true)
    oWidget.CustomData.bReceiveInput = true
    for nIndex, oChild in pairs(oWidget:GetChildren()) do
      oChild:SetEnabled(true)
    end
  elseif "Exit" == sStateAction then
    oWidget:SetVisible(false)
    oWidget.CustomData.bReceiveInput = false
    for nIndex, oChild in pairs(oWidget:GetChildren()) do
      oChild:SetEnabled(false)
    end
    tEvent = {}
    tEvent.EventType = "ImposterShellEvent"
    tEvent.bOn = false
    MrxGuiBase.SentEvent(tEvent)
  end
  MrxGuiBase.ChangeScreenResolution()
end

function HandleImposterInputEvent(oWidget, tEvent)
  if oWidget.CustomData.bReceiveInput and MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress then
    Sys.RequestGameState("ingame")
  end
end

function HandleImposterEvent(oWidget, tEvent)
  if tEvent.bOn then
    oWidget.CustomData.bImposterEnabled = true
  else
    oWidget.CustomData.bImposterEnabled = false
  end
end

function _LTIFscommand(oFlash, sFuncName)
  if sFuncName == "LTIVideoEnter" then
    LTILibName.LTIVideoEnter()
  elseif sFuncName == "LTIVideoAdvanceEnter" then
    LTILibName.LTIVideoAdvanceEnter()
  elseif sFuncName == "LTIInputGeneralEnter" then
    LTILibName.LTIInputGeneralEnter()
  elseif sFuncName == "LTIVideoSwitchMode" then
    LTILibName.LTIVideoSwitchMode()
  elseif sFuncName == "LTIVideoNextRes" then
    LTILibName.LTIVideoNextRes()
  elseif sFuncName == "LTIVideoPrevRes" then
    LTILibName.LTIVideoPrevRes()
  elseif sFuncName == "LTIVideoNextRefresh" then
    LTILibName.LTIVideoNextRefresh()
  elseif sFuncName == "LTIVideoPrevRefresh" then
    LTILibName.LTIVideoPrevRefresh()
  elseif sFuncName == "LTIVideoApplyChanges" then
    LTILibName.LTIVideoApplyChanges()
  elseif sFuncName == "LTIVideoCancel" then
    LTILibName.LTIVideoCancel()
  elseif sFuncName == "LTIVideoAdvanceEnter" then
    LTILibName.LTIVideoAdvanceEnter()
  elseif sFuncName == "LTIVideoAdvanceDefault" then
    LTILibName.LTIVideoAdvanceDefault()
  elseif sFuncName == "LTIInputGeneralEnter" then
    LTILibName.LTIInputGeneralEnter()
  elseif sFuncName == "LTIInputKMEnter" then
    LTILibName.LTIInputKMEnter()
  elseif sFuncName == "LTIInputKMApplyChanges" then
    LTILibName.LTIInputKMApplyChanges()
  elseif sFuncName == "LTIInputKMDefault" then
    LTILibName.LTIInputKMDefault()
  elseif sFuncName == "LTIInputKMCancelInput" then
    LTILibName.LTIInputKMCancelInput()
  elseif sFuncName == "LTIInputKMExit" then
    LTILibName.LTIInputKMExit()
  elseif sFuncName == "LTIInputJoystickEnter" then
    LTILibName.LTIInputJoystickEnter()
  elseif sFuncName == "LTIInputJoystickApplyChanges" then
    LTILibName.LTIInputJoystickApplyChanges()
  elseif sFuncName == "LTIInputJoystickCancel" then
    LTILibName.LTIInputJoystickCancel()
  elseif sFuncName == "LTIInputJoystickDefault" then
    LTILibName.LTIInputJoystickDefault()
  elseif sFuncName == "LTIInputJoystickExit" then
    LTILibName.LTIInputJoystickExit()
  end
end

function _LTIEnter(oFlash, iNumber)
  if iNumber == "1" then
    LTILibName.LTIVideoEnter()
  elseif iNumber == "2" then
    LTILibName.LTIVideoAdvanceEnter()
  elseif iNumber == "3" then
    LTILibName.LTIInputGeneralEnter()
  end
end

function _LTIVideo(oFlash, iNumber)
  if iNumber == "2" then
    LTILibName.LTIVideoSwitchMode()
  elseif iNumber == "3" then
    LTILibName.LTIVideoNextRes()
  elseif iNumber == "4" then
    LTILibName.LTIVideoPrevRes()
  elseif iNumber == "5" then
    LTILibName.LTIVideoNextRefresh()
  elseif iNumber == "6" then
    LTILibName.LTIVideoPrevRefresh()
  elseif iNumber == "7" then
    LTILibName.LTIVideoApplyChanges()
  elseif iNumber == "8" then
    LTILibName.LTIVideoCancel()
  end
end

function _LTIVideoSetGamma(oFlash, fNumber)
  LTILibName.LTIVideoSetGamma(fNumber)
end

function _LTIVideoAdvanceEnter(oFlash, sUnused)
  LTILibName.LTIVideoAdvanceEnter()
end

function _LTIVideoSwitchOpt1(oFlash, iNumber)
  LTILibName.LTIVideoSwitchOpt1(iNumber)
end

function _LTIVideoAdvanceDefault(oFlash, sUnused)
  LTILibName.LTIVideoAdvanceDefault()
end

function _LTIInputGeneralEnter(oFlash, sUnused)
  LTILibName.LTIInputGeneralEnter()
end

function _LTIInputGeneralInvertMouse(oFlash, iNumber)
  LTILibName.LTIInputGeneralInvertMouse(iNumber)
end

function _LTIInputGeneralMouseSense(oFlash, fNumber)
  LTILibName.LTIInputGeneralMouseSense(fNumber)
end

function _LTIInputGeneralJoySense(oFlash, fNumber)
  LTILibName.LTIInputGeneralJoySense(fNumber)
end

function _LTIInputGeneralRumble(oFlash, bBoolean)
  LTILibName.LTIInputGeneralRumble(bBoolean)
end

function _LTIInputKMEnter(oFlash, sUnused)
  LTILibName.LTIInputKMEnter()
end

function _LTIInputKMChangeInput(oFlash, iNumber)
  LTILibName.LTIInputKMChangeInput(iNumber)
end

function _LTIInputKMApplyChanges(oFlash, sUnused)
  LTILibName.LTIInputKMApplyChanges()
end

function _LTIInputKMDefault(oFlash, sUnused)
  LTILibName.LTIInputKMDefault()
end

function _LTIInputKMCancelInput(oFlash, sUnused)
  LTILibName.LTIInputKMCancelInput()
end

function _LTIOverBoundResponse(oFlash, iNumber)
  LTILibName.LTIOverBoundResponse(iNumber)
end

function _LTIInputKMExit(oFlash, sUnused)
  LTILibName.LTIInputKMExit()
end

function _LTIInputJoystickEnter(oFlash, sUnused)
  LTILibName.LTIInputJoystickEnter()
end

function _LTIInputJoystickChangePrimary(oFlash, iNumber)
  LTILibName.LTIInputJoystickChangePrimary(iNumber)
end

function _LTIInputJoystickChangeInput(oFlash, iNumber)
  LTILibName.LTIInputJoystickChangeInput(iNumber)
end

function _LTIInputJoystickApplyChanges(oFlash, sUnused)
  LTILibName.LTIInputJoystickApplyChanges()
end

function _LTIInputJoystickCancel(oFlash, sUnused)
  LTILibName.LTIInputJoystickCancel()
end

function _LTIInputJoystickDefault(oFlash, sUnused)
  LTILibName.LTIInputJoystickDefault()
end

function _LTIInputJoystickExit(oFlash, sUnused)
  LTILibName.LTIInputJoystickExit()
end

function _LTIJoystickOverBoundResponse(oFlash, iNumber)
  LTILibName.LTIJoystickOverBoundResponse(iNumber)
end

function _LTIPauseItemChanged(oFlash, iNumber)
  LTILibName.LTIPauseItemChanged(iNumber)
end

function _LTICamera(oFlash, iNumber)
  LTILibName.LTICamera(iNumber)
end
