local L0_1, L1_1, L2_1
import("MrxGuiBase", false)
import("MrxGuiManager", false)
import("MrxGuiDialogBox", false)
import("MrxSound", false)
import("MrxPlayState", false)
import("WifMissionFlow", false)
import("MrxPlayer", false)
import("MrxTutorialManager", false)
import("MrxStatsManager", false)
import("MrxUtil", false)
L0_1 = {}
L0_1.BUTTON_PAD1_U = 1
L0_1.BUTTON_PAD1_D = 2
L0_1.BUTTON_PAD1_L = 3
L0_1.BUTTON_PAD1_R = 4
L0_1.BUTTON_PAD2_U = 5
L0_1.BUTTON_PAD2_D = 6
L0_1.BUTTON_PAD2_L = 7
L0_1.BUTTON_PAD2_R = 8
L0_1.BUTTON_L_STICK_L = 9
L0_1.BUTTON_L_STICK_R = 10
L0_1.BUTTON_L_STICK_U = 11
L0_1.BUTTON_L_STICK_D = 12
L0_1.BUTTON_R_STICK_L = 13
L0_1.BUTTON_R_STICK_R = 14
L0_1.BUTTON_R_STICK_U = 15
L0_1.BUTTON_R_STICK_D = 16
L0_1.BUTTON_ALT1_1 = 17
L0_1.BUTTON_ALT1_2 = 18
L0_1.BUTTON_ALT1_3 = 19
L0_1.BUTTON_ALT2_1 = 20
L0_1.BUTTON_ALT2_2 = 21
L0_1.BUTTON_ALT2_3 = 22
L0_1.BUTTON_SYS1 = 23
L0_1.BUTTON_SYS2 = 24
Joystick = L0_1
tControlMap = false

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = {}
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Support_Menu]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Support_Menu]"
  L1_2[Joystick.BUTTON_PAD1_L] = "[SHELL.Controls.Switch_Explosive]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Action]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Jump]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reload]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Melee]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.LeftRight]"
  L1_2[Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.UpDown]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Crouch]"
  L1_2[Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.Use_Explosive]"
  L1_2[Joystick.BUTTON_ALT1_3] = "[SHELL.Controls.Sprint]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Zoom]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.human = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.car = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Move]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Aim_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.tank = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_L] = "[SHELL.Controls.Winch]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Raise]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Lower]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.LeftRight]"
  L1_2[Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Move]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Lower]"
  L1_2[Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.Raise]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.helicopter = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "Seat Menu"
  L1_2[Joystick.BUTTON_PAD1_D] = "Seat Menu"
  L1_2[Joystick.BUTTON_PAD2_U] = "Exit"
  L1_2[Joystick.BUTTON_PAD2_D] = "Accelerate"
  L1_2[Joystick.BUTTON_PAD2_L] = "Decelerate"
  L1_2[Joystick.BUTTON_PAD2_R] = "Winch"
  L1_2[Joystick.BUTTON_L_STICK_L] = "Roll L/R"
  L1_2[Joystick.BUTTON_L_STICK_U] = "Pitch U/D"
  L1_2[Joystick.BUTTON_R_STICK_L] = "Turn L/R"
  L1_2[Joystick.BUTTON_R_STICK_U] = "Aim U/D"
  L1_2[Joystick.BUTTON_ALT1_1] = "Lower"
  L1_2[Joystick.BUTTON_ALT1_2] = "Raise"
  L1_2[Joystick.BUTTON_ALT1_3] = "Toggle VTOL"
  L1_2[Joystick.BUTTON_ALT2_1] = "Fire Weapon"
  L1_2[Joystick.BUTTON_ALT2_2] = "Switch Weapon"
  L1_2[Joystick.BUTTON_ALT2_3] = "Camera Mode"
  L1_2[Joystick.BUTTON_SYS1] = "Pause"
  L1_2[Joystick.BUTTON_SYS2] = "PDA"
  L0_2.jet = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.boat = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Climb_UD]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Look_LR]"
  L0_2.ladder = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Aim_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Camera_Mode]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.seat = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]"
  L1_2[Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.boost]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Jump]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.dlc_car_boost = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_R] = "[SHELL.Controls.Camera_Mode]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Look_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]"
  L1_2[Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Handbrake]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Jump]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.dlc_car_singleweaponjump = L1_2
  L1_2 = {}
  L1_2[Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Show_Cash_And_Fuel]"
  L1_2[Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Exit]"
  L1_2[Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Forward]"
  L1_2[Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reverse]"
  L1_2[Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Call_Allies]"
  L1_2[Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.TurnLR]"
  L1_2[Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.Move]"
  L1_2[Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.Aim_LR]"
  L1_2[Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.Aim_UD]"
  L1_2[Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.callstrike]"
  L1_2[Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]"
  L1_2[Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]"
  L1_2[Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Reset_Camera]"
  L1_2[Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]"
  L1_2[Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
  L0_2.dlc_tank_strike = L1_2
  tControlMap = L0_2
end

Init = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = MrxGuiDialogBox.oSystemDialogBoxFlash
  if L1_2 then
    return
  end
  L1_2 = A0_2.CustomData.bActive
  if L1_2 then
    return
  end
  L1_2 = A0_2.CustomData.bHaveFlash
  if not L1_2 then
    L1_2 = A0_2.CustomData.bLoading
    if not L1_2 then
      L1_2 = A0_2.CustomData.oMapFlash
      MrxGuiBase.AddWidget(L1_2)
      L2_2 = A0_2.CustomData
      L2_2.bLoading = true
      L6_2 = {}
      L6_2[1] = A0_2
      L1_2.SetSwfFile(L1_2, L1_2.CustomData.sFile, _FinishPauseOpen, L6_2)
    end
  end
end

OpenPauseScreen = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2
  _FinishLoad(A0_2)
  MrxSound.EnterPauseState()
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.Restart(L1_2)
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.Play(L1_2)
  L1_2 = A0_2.CustomData
  L1_2.bActive = true
  MrxGuiBase.GetControlFocus(A0_2, true)
  A0_2.SetVisible(A0_2, true)
  L1_2 = A0_2.GetChildren(A0_2)
  L2_2 = pairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    MrxGuiBase.AddWidgetWithChildren(L6_2)
  end
  L2_2 = A0_2.CustomData.oMapFlash
  L3_2 = MrxPlayState.Get() == MrxPlayState._knMission
  L4_2 = WifMissionFlow.GetLastCompletedContractName()
  if not L4_2 then
    L4_2 = "none"
  end
  L10_2 = MrxPlayState.GetTotalTimeElapsed()
  L11_2 = MrxStatsManager.GetPercentCompleted
  L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2 = L11_2()
  _GuiInternal.SetFlashPauseMenu(L2_2.BasicData.uId, L3_2, L4_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2)
  L6_2 = Player.GetLocalPlayer()
  if L6_2 then
    L7_2 = MrxGuiManager.GetHudState(L6_2)
    L8_2 = A0_2.CustomData.tHudStates
    L8_2[L6_2] = L7_2
    if L7_2 then
      MrxGuiManager.ToggleHud(L6_2, false)
    end
    L9_2 = tControlMap[Player.GetControlBindingType(L6_2)]
    if not L9_2 then
      L9_2 = {}
    end
    L11_2 = Player.GetLocalCharacter
    L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2 = L11_2()
    L10_2 = Vehicle.GetFromRider(L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2)
    if L8_2 == "car" then
      L11_2 = Pg.GetGuidByName("BombCar")
      if L10_2 == L11_2 then
        L9_2 = tControlMap.dlc_car_boost
      else
        L11_2 = Object.GetParent(L10_2)
        L12_2 = Pg.GetGuidByName("Panhard (DLCCON004)")
        if L11_2 == L12_2 then
          L9_2 = tControlMap.dlc_car_singleweaponjump
        end
      end
    elseif L8_2 == "tank" then
      L11_2 = Object.GetParent(L10_2)
      L12_2 = Pg.GetGuidByName("DLC_M1A3")
      if L11_2 == L12_2 then
        L9_2 = tControlMap.dlc_tank_strike
      end
    end
    L11_2 = pairs
    L12_2 = L9_2
    L11_2, L12_2, L13_2 = L11_2(L12_2)
    for L14_2, L15_2 in L11_2, L12_2, L13_2 do
      L19_2 = {}
      L19_2[1] = L14_2
      L19_2[2] = L15_2
      L2_2.CallActionScriptCallback(L2_2, "controllerDisplay", L19_2)
    end
  end
  L7_2 = true
  L8_2 = Sys.SubtitlesEnabled
  if L8_2 then
    L7_2 = Sys.SubtitlesEnabled()
  end
  L11_2 = {}
  L11_2[1] = L7_2
  L2_2.CallActionScriptCallback(L2_2, "videoSubtitles", L11_2)
  L8_2 = true
  L9_2 = Sys.RumbleEnabled
  if L9_2 then
    L8_2 = Sys.RumbleEnabled()
  end
  L12_2 = {}
  L12_2[1] = L8_2
  L2_2.CallActionScriptCallback(L2_2, "gameRumble", L12_2)
  bTutorials = true
  L9_2 = Sys.TutorialsEnabled
  if L9_2 then
    bTutorials = Sys.TutorialsEnabled()
  end
  L12_2 = {}
  L12_2[1] = bTutorials
  L2_2.CallActionScriptCallback(L2_2, "gameTutorials", L12_2)
  L9_2 = false
  L10_2 = Sys.YAxisInverted
  if L10_2 then
    L9_2 = Sys.YAxisInverted()
  end
  L13_2 = {}
  L13_2[1] = L9_2
  L2_2.CallActionScriptCallback(L2_2, "gameinvert", L13_2)
  L10_2 = false
  L11_2 = MrxPlayState.IsFree()
  if not L11_2 then
    L10_2 = true
  end
  L14_2 = {}
  L14_2[1] = L10_2
  L2_2.CallActionScriptCallback(L2_2, "activeContract", L14_2)
  L11_2 = MrxPlayer.CanMedEvac()
  L15_2 = {}
  L15_2[1] = L11_2
  L2_2.CallActionScriptCallback(L2_2, "medevacEnable", L15_2)
  L12_2 = A0_2.CustomData.bSaveDisabled
  if L12_2 then
    L15_2 = {}
    L15_2[1] = true
    L2_2.CallActionScriptCallback(L2_2, "disableSave", L15_2)
  end
end

_FinishPauseOpen = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = A0_2.CustomData.bActive
  if not L1_2 then
    return
  end
  L1_2 = A0_2.CustomData
  L1_2.bActive = false
  MrxGuiBase.ReleaseControlFocus(A0_2)
  L1_2 = A0_2.CustomData.bHaveFlash
  if L1_2 then
    L1_2 = A0_2.CustomData.oMapFlash
    L1_2.CallActionScriptCallback(L1_2, "saveProfile", {})
    L1_2 = A0_2.CustomData.oMapFlash
    L1_2.Pause(L1_2)
    L1_2 = A0_2.CustomData.oMapFlash
    L1_2.SetSwfFile(L1_2, nil)
    L1_2 = A0_2.CustomData
    L1_2.bHaveFlash = false
  else
    L1_2 = A0_2.CustomData.bLoading
    if L1_2 then
      L1_2 = A0_2.CustomData.oMapFlash
      L1_2.SetSwfFile(L1_2, nil)
      L1_2 = A0_2.CustomData
      L1_2.bLoading = false
    end
  end
  A0_2.SetVisible(A0_2, false)
  L1_2 = A0_2.GetChildren(A0_2)
  L2_2 = pairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    MrxGuiBase.RemoveWidgetWithChildren(L6_2)
  end
  L2_2 = Player.GetLocalPlayer()
  if L2_2 then
    L3_2 = A0_2.CustomData.tHudStates[L2_2]
    if L3_2 then
      MrxGuiManager.ToggleHud(L2_2, true)
      L3_2 = A0_2.CustomData.tHudStates
      L3_2[L2_2] = nil
    end
  end
  MrxSound.ExitPauseState()
  L3_2 = 3
  L4_2 = {}
  L4_2.EventType = "GuiShowAmmoCounter"
  L4_2.bShowGun = true
  L4_2.bShowExplosive = true
  L4_2.nTime = L3_2
  MrxGuiBase.SentEvent(L4_2)
  L5_2 = {}
  L5_2.EventType = "ShowAllCounters"
  L5_2.nTime = L3_2
  MrxGuiBase.SentEvent(L5_2)
  L6_2 = bTutorials
  if L6_2 == true then
    L6_2 = Sys.TutorialsEnabled()
    if L6_2 == false then
      MrxTutorialManager.HideMessage(true)
    end
  end
end

ClosePauseScreen = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  if nil ~= A1_2 then
    L2_2 = A0_2.CustomData
    L2_2.bSaveDisabled = not A1_2
  end
end

SetUserSaveEnabled = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  L3_2 = A0_2.CustomData.bImposterEnabled
  if L3_2 then
    return
  end
  if not A1_2 or not A2_2 then
    return
  end
  if "Pause" ~= A1_2 then
    return
  end
  if "Enter" == A2_2 then
    OpenPauseScreen(A0_2)
  elseif "Exit" == A2_2 then
    ClosePauseScreen(A0_2)
  end
  MrxGuiBase.ChangeScreenResolution()
end

HandleStateChangeEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  A0_2.SetUseImmortalEvents(A0_2, true)
  L2_2 = A0_2.CustomData
  L2_2.oMenu = A0_2.GetChildren(A0_2)[3]
  L2_2 = A0_2.GetChildren(A0_2)[1]
  L2_2.SetFullscreen(L2_2, true)
  ClosePauseScreen(A0_2)
  A0_2.SetEventHandler(A0_2, "GuiStateChangeEvent", nil)
  A0_2.SetEventHandler(A0_2, "GuiStateChangeEvent", HandleStateChangeEvent)
  L2_2 = A0_2.CustomData
  L2_2.tHudStates = {}
end

HandleInitializationEvent = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  A0_2.SetUseImmortalEvents(A0_2, true)
  L1_2 = A0_2.CustomData
  L1_2.bActive = true
  L1_2 = MrxGuiBase.ImageWidget
  L1_2 = L1_2.new(L1_2)
  L1_2.SetTransient(L1_2, false)
  L1_2.SetFullscreen(L1_2, true)
  L1_2.SetColor(L1_2, 0, 0, 0, 192)
  A0_2.AddChild(A0_2, L1_2)
  L1_2.oParentWidget = A0_2
  L2_2 = MrxGuiBase.FlashWidget
  L2_2 = L2_2.new(L2_2)
  L2_2.SetTransient(L2_2, false)
  L2_2.SetAnchoring(L2_2, "center", "center")
  L3_2 = 283.33334
  L2_2.SetLocation(L2_2, (320 - L3_2), 0, (320 + L3_2), 480)
  A0_2.AddChild(A0_2, L2_2)
  L4_2 = A0_2.CustomData
  L4_2.oMapFlash = L2_2
  L4_2 = L2_2.CustomData
  L4_2.sFile = "pause_menu"
  L2_2.oParentWidget = A0_2
  A0_2.Open = OpenPauseScreen
  A0_2.Close = ClosePauseScreen
  A0_2.SetUserSaveEnabled = SetUserSaveEnabled
  A0_2.SetEventHandler(A0_2, "ControllerInput", _HandleInput)
  A0_2.nAnalogInputHeld = 0
  L4_2 = A0_2.CustomData
  L4_2.tHudStates = {}
  Pg.LoadAsset("pause_graphic", "texture")
  A0_2.Close(A0_2)
end

_Initialize = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = A0_2.CustomData
  L1_2.bHaveFlash = true
  L1_2 = A0_2.CustomData
  L1_2.bLoading = false
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "quitGame", _HandleQuitEvent)
  L1_2.SetFlashEventHandler(L1_2, "closePause", _HandleCloseEvent)
  L1_2.SetFlashEventHandler(L1_2, "messageMedEvac", _ConfirmMedEvacEvent)
  L1_2.SetFlashEventHandler(L1_2, "messageButton", _HandleMedEvacEvent, {})
  L1_2.Pause(L1_2)
end

_FinishLoad = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = A0_2.CustomData.bActive
  if L2_2 then
    A0_2.Close(A0_2)
  else
    A0_2.Open(A0_2)
  end
end

_HandleToggleEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2.CustomData.oMapFlash
  L2_2.EventHandlers.ControllerInput(L2_2, A1_2)
end

_HandleInput = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L2_2 = A0_2.oParentWidget.CustomData.bActive
  if L2_2 then
    Sys.RequestGameState("ingame")
    L1_2.Close(L1_2)
  end
end

_HandleCloseEvent = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = A0_2.oParentWidget
  Sys.RequestGameState("unloading")
  Net.QuitGame()
  L1_2.Close(L1_2)
end

_HandleQuitEvent = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L2_2 = MrxPlayer.GetMedEvacCost
  L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L2_2()
  L1_2 = MrxUtil.FormatMoney(L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
  L5_2 = {}
  L6_2 = "[SHELL.Confirmation.AreYouSure]"
  L7_2 = "[PauseMenu.Base.MedEvacMessage] " .. L1_2 .. " [PauseMenu.Base.MedEvacMessageEnd]"
  L9_2 = "[SHELL.Confirmation.Yes]"
  L10_2 = "[SHELL.Confirmation.No]"
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = 2
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  L5_2[6] = "messageMedEvac"
  L5_2[7] = true
  A0_2.CallActionScriptCallback(A0_2, "onlineMessage", L5_2)
  _bMedEvac = true
end

_ConfirmMedEvacEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = _bMedEvac
  if not L2_2 then
    return
  end
  _bMedEvac = nil
  A0_2.CallActionScriptCallback(A0_2, "onlineMessageClose", {})
  if A1_2 == "1" then
    MrxPlayer.MedEvac()
    Sys.RequestGameState("ingame")
  end
end

_HandleMedEvacEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2.GetChildren(A0_2)[1]
  L2_2.SetFullscreen(L2_2, true)
  A0_2.SetVisible(A0_2, false)
  L2_2 = A0_2.CustomData
  L2_2.bReceiveInput = false
  L2_2 = pairs
  L4_2 = A0_2
  L3_2 = A0_2.GetChildren
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2 = L3_2(L4_2)
  L2_2, L3_2, L4_2 = L2_2(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L6_2.SetEnabled(L6_2, false)
  end
end

HandleImposterInitializationEvent = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = A0_2.CustomData.bImposterEnabled
  if not L3_2 then
    return
  end
  if not A1_2 or not A2_2 then
    return
  end
  if "Pause" ~= A1_2 then
    return
  end
  if "Enter" == A2_2 then
    A0_2.SetVisible(A0_2, true)
    L3_2 = A0_2.CustomData
    L3_2.bReceiveInput = true
    L3_2 = pairs
    L5_2 = A0_2
    L4_2 = A0_2.GetChildren
    L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2 = L4_2(L5_2)
    L3_2, L4_2, L5_2 = L3_2(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L7_2.SetEnabled(L7_2, true)
    end
  elseif "Exit" == A2_2 then
    A0_2.SetVisible(A0_2, false)
    L3_2 = A0_2.CustomData
    L3_2.bReceiveInput = false
    L3_2 = pairs
    L5_2 = A0_2
    L4_2 = A0_2.GetChildren
    L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2 = L4_2(L5_2)
    L3_2, L4_2, L5_2 = L3_2(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L7_2.SetEnabled(L7_2, false)
    end
    tEvent = {}
    L3_2 = tEvent
    L3_2.EventType = "ImposterShellEvent"
    L3_2 = tEvent
    L3_2.bOn = false
    MrxGuiBase.SentEvent(tEvent)
  end
  MrxGuiBase.ChangeScreenResolution()
end

HandleImposterStateChangeEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = A0_2.CustomData.bReceiveInput
  if L2_2 then
    L2_2 = MrxGuiBase.Joystick.BUTTON_PAD2_D
    L3_2 = A1_2.ButtonPress
    if L2_2 == L3_2 then
      Sys.RequestGameState("ingame")
    end
  end
end

HandleImposterInputEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2
  L2_2 = A1_2.bOn
  if L2_2 then
    L2_2 = A0_2.CustomData
    L2_2.bImposterEnabled = true
  else
    L2_2 = A0_2.CustomData
    L2_2.bImposterEnabled = false
  end
end

HandleImposterEvent = L0_1
