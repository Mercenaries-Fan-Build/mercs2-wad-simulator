local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1
import("MrxGuiBase", false)
import("MrxPmc", false)
import("MrxGuiManager", false)
import("WifVzRegionNames", false)
import("MrxSupportData", false)
import("MrxGuiDialogBox", false)
import("MrxSound", false)
import("MrxPlayState", false)
import("WifMissionData", false)
import("WifMissionFlow", false)
import("MrxGuiHudFactionGauge", false)
import("MrxStatsManager", false)
import("MrxState", false)
import("WifPmcInterior", false)
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_ALT1_2
L0_1[2] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[3] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[4] = MrxGuiBase.Joystick.BUTTON_ALT1_2
L0_1[5] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[6] = MrxGuiBase.Joystick.BUTTON_ALT1_2
L0_1[7] = MrxGuiBase.Joystick.BUTTON_ALT1_2
L0_1[8] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[9] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[10] = MrxGuiBase.Joystick.BUTTON_ALT2_2
L0_1[11] = MrxGuiBase.Joystick.BUTTON_ALT1_2
_tCheatEnable = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_R
_tCheatInvincible = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_L
_tCheatInfiniteAmmo = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_D
_tCheatFuel = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[7] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[8] = MrxGuiBase.Joystick.BUTTON_PAD1_U
_tCheatAirstrike = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[7] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[8] = MrxGuiBase.Joystick.BUTTON_PAD1_R
_tCheatNuke = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[7] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[8] = MrxGuiBase.Joystick.BUTTON_PAD1_U
_tCheatSupplies = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[6] = MrxGuiBase.Joystick.BUTTON_PAD1_L
_tCheatVehicles = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_U
_tCheatCostumes = L0_1
L0_1 = {}
L0_1[1] = MrxGuiBase.Joystick.BUTTON_PAD1_U
L0_1[2] = MrxGuiBase.Joystick.BUTTON_PAD1_L
L0_1[3] = MrxGuiBase.Joystick.BUTTON_PAD1_D
L0_1[4] = MrxGuiBase.Joystick.BUTTON_PAD1_R
L0_1[5] = MrxGuiBase.Joystick.BUTTON_PAD1_U
_tCheatGrapple = L0_1
_sCheatSound = "UI_SatDes_Circular_Timing_Correct"

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Player.GetLocalCharacter()
  if L0_2 then
    L1_2 = not Object.GetInvincible(L0_2, "Cheat")
    L2_2 = Player.GetPrimaryCharacter()
    if L2_2 then
      Object.SetInvincible(Player.GetPrimaryCharacter(), L1_2, "Cheat")
    end
    L2_2 = Player.GetSecondaryCharacter()
    if L2_2 then
      Object.SetInvincible(Player.GetSecondaryCharacter(), L1_2, "Cheat")
    end
    if L1_2 then
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INVINCIBLE_ON, {}, true)
    else
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INVINCIBLE_OFF, {}, true)
    end
  end
end

_CheatToggleInvincible = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L0_2 = Object.GetInfiniteAmmo
  if not L0_2 then
    return
  end
  L0_2 = Player.GetLocalCharacter()
  if L0_2 then
    L1_2 = not Object.GetInfiniteAmmo(L0_2)
    Object.SetInfiniteAmmo(L0_2, L1_2)
    _SendInfiniteAmmoEvent(L1_2)
  end
end

_CheatToggleInfiniteAmmo = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = Player.GetLocalCharacter() == Player.GetPrimaryCharacter
  if L2_2 then
    L3_2 = nOn
    if L3_2 then
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_ON, {}, true)
    else
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_OFF, {}, true)
    end
  else
    L3_2 = nOn
    if L3_2 then
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_ON, {}, true)
    else
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_OFF, {}, true)
    end
  end
end

_SendInfiniteAmmoEvent = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = MrxPmc.AddFuelQty
  L1_2 = MrxPmc.GetFuelCapacity()
  if not L1_2 then
    L1_2 = 300
  end
  L0_2(L1_2)
end

_CheatAddFuel = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  MrxPmc.AddSupportQty("laserguidedbomb", 25, false, 0)
  MrxPmc.AddSupportQty("moab", 25, false, 0)
  MrxPmc.AddSupportQty("rocketartillery", 25, false, 0)
  MrxPmc.AddSupportQty("smartbomb", 25, false, 0)
  MrxPmc.AddSupportQty("strategicmissile", 25, false, 0)
  MrxPmc.AddSupportQty("surgicalstrike", 25, false, 0)
  MrxPmc.AddSupportQty("tankbuster", 25, false, 0)
  MrxPmc.AddSupportQty("artillery", 25, false, 0)
  MrxPmc.AddSupportQty("bombingrun", 25, false, 0)
  MrxPmc.AddSupportQty("bunkerbuster", 25, false, 0)
  MrxPmc.AddSupportQty("carpetbomb", 25, false, 0)
  MrxPmc.AddSupportQty("clusterbomb", 25, false, 0)
  MrxPmc.AddSupportQty("combatairpatrol", 25, false, 0)
  MrxPmc.AddSupportQty("cruisemissile", 25, false, 0)
  MrxPmc.AddSupportQty("daisycutter", 25, false, 0)
  MrxPmc.AddSupportQty("fuelairbomb", 25, false, 0)
end

_CheatAddAirstrikes = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  MrxPmc.AddSupportQty("nuke", 25, false, 0)
end

_CheatAddNuke = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L0_2 = pairs
  L1_2 = MrxSupportData.tSupportData
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L5_2 = L4_2.sType
    if "Supply" == L5_2 then
      MrxPmc.AddSupportQty(L3_2, 25, false, 0)
    end
  end
end

_CheatAddSupplies = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L0_2 = pairs
  L1_2 = MrxSupportData.tSupportData
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L5_2 = L4_2.sType
    if "Heavy" ~= L5_2 then
      L5_2 = L4_2.sType
      if "Light" ~= L5_2 then
        L5_2 = L4_2.sType
        if "Civilian" ~= L5_2 then
          L5_2 = L4_2.sType
          if "Heli" ~= L5_2 then
            L5_2 = L4_2.sType
            if "Boat" ~= L5_2 then
              goto lbl_28
            end
          end
        end
      end
    end
    MrxPmc.AddSupportQty(L3_2, 25, false, 0)
    ::lbl_28::
  end
end

_CheatAddVehicles = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = Net.IsClient()
  if L0_2 then
    Player.SetAvailableCostumes(-1)
  else
    WifPmcInterior.SetAvailableCostumes(-1)
  end
end

_CheatAddCostumes = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L0_2 = Net.IsClient()
  if L0_2 then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_GRAPPLE_ON, {}, true)
  else
    WifMissionFlow.SetGrappleEnabled(true)
  end
end

_CheatUnlockGrapple = L0_1
_kMaxCodeLength = 12

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = type(A0_2)
  if "table" ~= L2_2 then
    L2_2 = false
    return L2_2
  end
  L2_2 = type(A1_2)
  if "table" ~= L2_2 then
    L2_2 = false
    return L2_2
  end
  L2_2 = 1
  while true do
    L3_2 = _kMaxCodeLength
    if not (L2_2 <= L3_2) then
      break
    end
    L3_2 = type(A0_2[L2_2])
    L4_2 = type(A1_2[L2_2])
    if L3_2 ~= L4_2 then
      L3_2 = false
      return L3_2
    end
    L3_2 = A0_2[L2_2]
    L4_2 = A1_2[L2_2]
    if L3_2 ~= L4_2 then
      L3_2 = false
      return L3_2
    end
    L2_2 = L2_2 + 1
  end
  L3_2 = true
  return L3_2
end

_CheckCode = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = {}
  L1_2[1] = 0
  L1_2[2] = 0
  L1_2[3] = 0
  L1_2[4] = 0
  L1_2[5] = 0
  L1_2[6] = 0
  L1_2[7] = 0
  L1_2[8] = 0
  L1_2[9] = 0
  L1_2[10] = 0
  A0_2 = L1_2
end

_InvalidateCode = L0_1
NETEVENT_SETSELECTEDMISSION = 0
NETEVENT_PDAOPEN = 1
NETEVENT_PDACLOSE = 2
NETEVENT_CHEATS_ENABLED = 3
NETEVENT_CHEAT_INVINCIBLE_ON = 4
NETEVENT_CHEAT_INVINCIBLE_OFF = 5
NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_ON = 6
NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_OFF = 7
NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_ON = 8
NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_OFF = 9
NETEVENT_CHEAT_GRAPPLE_ON = 10

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L2_2 = NETEVENT_SETSELECTEDMISSION
  if A0_2 == L2_2 then
    L4_2 = Player.GetLocalPlayer
    L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2 = L4_2()
    L2_2 = MrxGuiBase.GetWidgetByNameAndOwner("PDA", L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2)
    if L2_2 then
      L3_2 = nil
      L4_2 = A1_2[1]
      if L4_2 then
        L3_2 = WifMissionData.GetMissionIdFromIndex(A1_2[1])
      end
      L2_2.SetSelectedMission(L2_2, L3_2, true)
    else
      L5_2 = {}
      L5_2[1] = 1
      L7_2 = {}
      L7_2[1] = A0_2
      L7_2[2] = A1_2
      Event.Create(Event.TimerRelative, L5_2, NetEventCallback, L7_2)
    end
  else
    L2_2 = NETEVENT_PDAOPEN
    if A0_2 == L2_2 then
      L4_2 = {}
      L4_2.uPlayer = Player.GetLocalPlayer()
      Event.Post("PDA Open", L4_2)
    else
      L2_2 = NETEVENT_PDACLOSE
      if A0_2 == L2_2 then
        Event.Post("PDA Close", {})
      end
    end
  end
  L2_2 = Pg.SetCheatsUsed
  if not L2_2 then
    return
  end
  L2_2 = A0_2
  if L2_2 then
    L3_2 = NETEVENT_CHEATS_ENABLED
    if L2_2 == L3_2 then
      L3_2 = Pg.WereCheatsUsed()
      if not L3_2 then
        L3_2 = Net.IsClient()
        if L3_2 then
          L3_2 = Player.GetLocalPlayer()
          if L3_2 then
            L3_2 = nil
            L4_2 = Sys.GetPlatform()
            if 1 == L4_2 then
              L3_2 = "[Patch2.Cheats.WarningPS3Client]"
            else
              L4_2 = Sys.GetPlatform()
              if 2 == L4_2 then
                L3_2 = "[Patch2.Cheats.Warning360Client]"
              end
            end
            L4_2 = MrxGuiDialogBox.DisplayDialogBox
            L5_2 = Player.GetLocalPlayer()
            L6_2 = L3_2 or L6_2
            if not L3_2 then
              L6_2 = "Warning: Cheats enabled by host."
            end
            L7_2 = {}
            L7_2[1] = "[Generic.Continue]"
            L4_2(L5_2, L6_2, L7_2, 1, nil, nil, nil, nil, nil, nil, nil, nil)
          end
        end
      end
      Pg.SetCheatsUsed(true)
  end
  elseif L2_2 then
    L3_2 = NETEVENT_CHEAT_INVINCIBLE_ON
    if L2_2 == L3_2 then
      L3_2 = Player.GetPrimaryCharacter()
      if L3_2 then
        Object.SetInvincible(Player.GetPrimaryCharacter(), true)
      end
      L3_2 = Player.GetSecondaryCharacter()
      if L3_2 then
        Object.SetInvincible(Player.GetSecondaryCharacter(), true)
      end
    else
      L3_2 = NETEVENT_CHEAT_INVINCIBLE_OFF
      if L2_2 == L3_2 then
        L3_2 = Player.GetPrimaryCharacter()
        if L3_2 then
          Object.SetInvincible(Player.GetPrimaryCharacter(), false)
        end
        L3_2 = Player.GetSecondaryCharacter()
        if L3_2 then
          Object.SetInvincible(Player.GetSecondaryCharacter(), false)
        end
      else
        L3_2 = NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_ON
        if L2_2 == L3_2 then
          L3_2 = Player.GetPrimaryCharacter()
          if L3_2 then
            Object.SetInfiniteAmmo(Player.GetPrimaryCharacter(), true)
          end
        else
          L3_2 = NETEVENT_CHEAT_INFINITE_AMMO_PRIMARY_OFF
          if L2_2 == L3_2 then
            L3_2 = Player.GetPrimaryCharacter()
            if L3_2 then
              Object.SetInfiniteAmmo(Player.GetPrimaryCharacter(), false)
            end
          else
            L3_2 = NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_ON
            if L2_2 == L3_2 then
              L3_2 = Player.GetSecondaryCharacter()
              if L3_2 then
                Object.SetInfiniteAmmo(Player.GetSecondaryCharacter(), true)
              end
            else
              L3_2 = NETEVENT_CHEAT_INFINITE_AMMO_SECONDARY_OFF
              if L2_2 == L3_2 then
                L3_2 = Player.GetSecondaryCharacter()
                if L3_2 then
                  Object.SetInfiniteAmmo(Player.GetSecondaryCharacter(), false)
                end
              else
                L3_2 = NETEVENT_CHEAT_GRAPPLE_ON
                if L2_2 == L3_2 then
                  L3_2 = Net.IsClient()
                  if not L3_2 then
                    WifMissionFlow.SetGrappleEnabled(true)
                  end
                end
              end
            end
          end
        end
      end
    end
  end
end

NetEventCallback = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Pg.WereCheatsUsed
  if L0_2 then
    L0_2 = Pg.WereCheatsUsed()
    if L0_2 then
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEATS_ENABLED, {}, true)
  end
  else
    return
  end
  L1_2 = Object.GetInvincible(Player.GetLocalCharacter(), "Cheat")
  if L1_2 then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEAT_INVINCIBLE_ON, {}, true)
    L2_2 = Player.GetPrimaryCharacter()
    if L2_2 then
      Object.SetInvincible(Player.GetPrimaryCharacter(), L1_2, "Cheat")
    end
    L2_2 = Player.GetSecondaryCharacter()
    if L2_2 then
      Object.SetInvincible(Player.GetSecondaryCharacter(), L1_2, "Cheat")
    end
  end
  L2_2 = Object.GetInfiniteAmmo
  if L2_2 then
    L2_2 = Object.GetInfiniteAmmo(L0_2)
  end
  if L2_2 then
    _SendInfiniteAmmoEvent(L2_2)
  end
end

OnPlayerJoin = L0_1
_knBlipLimit = 5000

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = A0_2.CustomData.bActive
  if L1_2 then
    Sound.CueSound(0, "ui_PDA_Open_01_st")
  end
end

_PlayDelayedOpenSound = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L4_2 = {}
  L4_2[1] = A0_2
  L4_2[2] = true
  L6_2 = {}
  L6_2[1] = A1_2
  Event.Create(Event.TimerRelative, L4_2, _PlayDelayedOpenSound, L6_2)
end

_SetupDelayedOpenSound = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = A0_2.CustomData.bActive
  if L1_2 then
    return
  end
  L1_2 = A0_2.CustomData.bHaveFlash
  if not L1_2 then
    return
  end
  L1_2 = Net.IsClient()
  if L1_2 then
    SetMissionChangeAllowed(A0_2, false)
  end
  L1_2 = MrxGuiDialogBox.oSystemDialogBoxFlash
  if L1_2 then
    return
  end
  L1_2 = MrxState.IsLocked()
  if L1_2 then
    return
  end
  L4_2 = A0_2
  L3_2 = A0_2.GetOwner
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L3_2(L4_2)
  L1_2 = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
  if L1_2 then
    L2_2 = L1_2.CustomData.bEnabled
    if L2_2 then
      L1_2.Close(L1_2)
    end
  end
  L4_2 = A0_2
  L3_2 = A0_2.GetOwner
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L3_2(L4_2)
  L2_2 = MrxGuiBase.GetCurrentControlHolder(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
  if L2_2 then
    return
  end
  L2_2 = A0_2.CustomData.nSuppressedCount
  if 0.5 < L2_2 then
    return
  end
  L2_2 = A0_2.CustomData.nCooldownFrames
  if 0 < L2_2 then
    return
  end
  L2_2 = A0_2.CustomData.oMapFlash
  L2_2.Restart(L2_2)
  L2_2 = A0_2.CustomData.oMapFlash
  L2_2.Play(L2_2)
  L2_2 = A0_2.CustomData
  L2_2.bActive = true
  A0_2.nAnalogInputHeld = 0
  L2_2 = A0_2.CustomData
  L2_2.tControlHistory = {}
  MrxGuiBase.GetControlFocus(A0_2, true)
  L2_2 = A0_2.CustomData.oMapFlash.BasicData.uId
  if L2_2 then
    _GuiInternal.RegisterForPdaUpdate(A0_2.CustomData.oMapFlash.BasicData.uId, true)
  end
  L5_2 = nil
  _PopulateMapDisplay(A0_2, nil, L5_2, _knBlipLimit, false)
  _PopulateSupportDisplay(A0_2)
  MrxStatsManager.BuildStats(A0_2)
  MrxStatsManager.PdaStatistics(A0_2)
  _PopulateDatabaseDisplay(A0_2)
  A0_2.SetVisible(A0_2, true)
  L2_2 = A0_2.GetChildren(A0_2)
  L3_2 = pairs
  L4_2 = L2_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L11_2 = A0_2
    L10_2 = A0_2.GetOwner
    L10_2, L11_2 = L10_2(L11_2)
    L7_2.SetOwner(L7_2, L10_2, L11_2)
    MrxGuiBase.AddWidgetWithChildren(L7_2)
  end
  L5_2 = A0_2
  L4_2 = A0_2.GetOwner
  L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L4_2(L5_2)
  L3_2 = MrxGuiManager.GetHudState(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
  L4_2 = A0_2.CustomData
  L4_2.bHudState = L3_2
  if L3_2 then
    MrxGuiManager.ToggleHud(A0_2.GetOwner(A0_2), false)
  end
  L4_2 = A0_2.CustomData.nFakePlayerX
  if not L4_2 then
    _GuiInternal.SetPlayerPDAWidget(A0_2.GetOwner(A0_2), A0_2.CustomData.oMapFlash.BasicData.uId)
  else
    _GuiInternal.SetPlayerPDAWidget(A0_2.GetOwner(A0_2), 0)
    Sys.RequestGameState("PDA")
  end
  A0_2.SetEventHandler(A0_2, "GuiUpdate", _HandlePDAUpdateEvent)
  _SetupDelayedOpenSound(0.5, A0_2)
  MrxSound.EnterPDAState()
  L4_2 = Sys.GetPlatform
  if L4_2 then
    L4_2 = Sys.GetPlatform()
    if 1 == L4_2 then
      L5_2 = A0_2.CustomData.oMapFlash
      L8_2 = {}
      L8_2[1] = "PS3"
      L5_2.CallActionScriptCallback(L5_2, "setPlatform", L8_2)
    elseif 2 == L4_2 then
      L5_2 = A0_2.CustomData.oMapFlash
      L8_2 = {}
      L8_2[1] = "360"
      L5_2.CallActionScriptCallback(L5_2, "setPlatform", L8_2)
    end
  end
  L6_2 = {}
  L6_2.uPlayer = A0_2.GetOwner(A0_2)
  Event.Post("PDA Open", L6_2)
  L4_2 = Net.IsClient()
  if L4_2 then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_PDAOPEN, {})
  end
end

Open = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L1_2 = A0_2.CustomData.oMapFlash.BasicData.uId
  if L1_2 then
    _GuiInternal.RegisterForPdaUpdate(A0_2.CustomData.oMapFlash.BasicData.uId, false)
  end
  L1_2 = A0_2.CustomData
  L1_2.bActive = false
  MrxGuiBase.ReleaseControlFocus(A0_2)
  L1_2 = A0_2.CustomData.bHaveFlash
  if L1_2 then
    L1_2 = A0_2.CustomData.oMapFlash
    L4_2 = {}
    L4_2[1] = true
    L1_2.CallActionScriptCallback(L1_2, "requestClose", L4_2)
  end
  A0_2.SetVisible(A0_2, false)
  L1_2 = A0_2.GetChildren(A0_2)
  L2_2 = pairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    MrxGuiBase.RemoveWidgetWithChildren(L6_2)
  end
  L2_2 = _GuiInternal.SetPlayerPDAWidget
  if L2_2 then
    _GuiInternal.SetPlayerPDAWidget(A0_2.GetOwner(A0_2), 0)
  end
  L2_2 = A0_2.CustomData.bHudState
  if L2_2 then
    MrxGuiManager.ToggleHud(A0_2.GetOwner(A0_2), true)
  end
  L2_2 = A0_2.CustomData.oSubtitle.ClearMessages
  if L2_2 then
    L2_2 = A0_2.CustomData.oSubtitle
    L2_2.ClearMessages(L2_2)
  end
  L2_2 = A0_2.CustomData.bMapMode
  if L2_2 then
    L2_2 = A0_2.CustomData
    L2_2.nFramesWithoutInput = -1
    L2_2 = A0_2.CustomData.oMapFlash
    L2_2.HandleLeftAnalogInput(L2_2, 0, 0)
    L2_2.HandleRightAnalogInput(L2_2, 0, 0)
  end
  L2_2 = A0_2.CustomData
  L2_2.nCooldownFrames = 20
  A0_2.SetEventHandler(A0_2, "GuiUpdate", _PdaCooldown)
  L2_2 = A0_2.CustomData.oTransit
  if L2_2 then
    _RemoveTransitInterface(A0_2)
  end
  Sound.CueSound(0, "ui_PDA_Close_01_st")
  MrxSound.ExitPDAState()
  L4_2 = {}
  L4_2.uPlayer = A0_2.GetOwner(A0_2)
  Event.Post("PDA Close", L4_2)
  L2_2 = Net.IsClient()
  if L2_2 then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_PDACLOSE, {})
  end
  L2_2 = A0_2.CustomData.oMapFlash
  L2_2.SetSwfFile(L2_2, nil)
  L3_2 = A0_2.CustomData
  L3_2.bHaveFlash = false
  MrxGuiBase.AddWidget(L2_2)
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2.SetSwfFile(L2_2, L2_2.CustomData.sFile, _FinishPdaReload, L7_2)
  A0_2.nAnalogInputHeld = 0
  MrxPmc.SetAllSupportViewed()
  L3_2 = A0_2.CustomData.oWindow
  if L3_2 then
    L3_2 = A0_2.CustomData.oWindow
    L3_2.SetText(L3_2, " ")
  end
  L3_2 = Pg.WereCheatsUsed
  if L3_2 then
    L3_2 = Pg.SetCheatsUsed
    if L3_2 then
      L3_2 = Pg.WereCheatsUsed()
      if L3_2 then
        L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatInvincible)
        if L3_2 then
          _CheatToggleInvincible()
          Sound.CueSound(0, _sCheatSound)
        else
          L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatInfiniteAmmo)
          if L3_2 then
            _CheatToggleInfiniteAmmo()
            Sound.CueSound(0, _sCheatSound)
          else
            L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatFuel)
            if L3_2 then
              _CheatAddFuel()
              Sound.CueSound(0, _sCheatSound)
            else
              L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatAirstrike)
              if L3_2 then
                _CheatAddAirstrikes()
                Sound.CueSound(0, _sCheatSound)
              else
                L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatNuke)
                if L3_2 then
                  _CheatAddNuke()
                  Sound.CueSound(0, _sCheatSound)
                else
                  L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatSupplies)
                  if L3_2 then
                    _CheatAddSupplies()
                    Sound.CueSound(0, _sCheatSound)
                  else
                    L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatVehicles)
                    if L3_2 then
                      _CheatAddVehicles()
                      Sound.CueSound(0, _sCheatSound)
                    else
                      L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatCostumes)
                      if L3_2 then
                        _CheatAddCostumes()
                        Sound.CueSound(0, _sCheatSound)
                      else
                        L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatGrapple)
                        if L3_2 then
                          _CheatUnlockGrapple()
                          Sound.CueSound(0, _sCheatSound)
                        end
                      end
                    end
                  end
                end
              end
            end
          end
        end
      else
        L3_2 = _CheckCode(A0_2.CustomData.tControlHistory, _tCheatEnable)
        if L3_2 then
          L3_2 = Net.IsClient()
          if not L3_2 then
            L3_2 = nil
            L4_2 = Sys.GetPlatform()
            if 1 == L4_2 then
              L3_2 = "[Patch2.Cheats.WarningPS3Host]"
            else
              L4_2 = Sys.GetPlatform()
              if 2 == L4_2 then
                L3_2 = "[Patch2.Cheats.Warning360Host]"
              end
            end
            L4_2 = MrxGuiDialogBox.DisplayDialogBox
            L6_2 = A0_2
            L5_2 = A0_2.GetOwner(L6_2)
            L6_2 = L3_2 or L6_2
            if not L3_2 then
              L6_2 = "Enable cheats?"
            end
            L7_2 = {}
            L8_2 = "[Generic.Accept]"
            L9_2 = "[Generic.Cancel]"
            L7_2[1] = L8_2
            L7_2[2] = L9_2
            L4_2(L5_2, L6_2, L7_2, 2, _CheatsEnableCallback, {}, nil, nil, nil, nil, true, 2)
          end
        end
      end
    end
  end
end

Close = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  if 1 == A0_2 then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_CHEATS_ENABLED, {}, true)
    Pg.SetCheatsUsed(true)
  end
end

_CheatsEnableCallback = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  _FinishLoad(A0_2)
  MrxGuiBase.RemoveWidget(A0_2.CustomData.oMapFlash)
end

_FinishPdaReload = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = A0_2.CustomData
  L1_2.nCooldownFrames = (A0_2.CustomData.nCooldownFrames - 1)
  L1_2 = A0_2.CustomData.nCooldownFrames
  if L1_2 <= 0 then
    L1_2 = A0_2.CustomData
    L1_2.nCooldownFrames = 0
    A0_2.SetEventHandler(A0_2, "GuiUpdate", nil)
  end
end

_PdaCooldown = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  if A1_2 then
    L2_2 = A0_2.CustomData
    L2_2.nSuppressedCount = (A0_2.CustomData.nSuppressedCount + 1)
  else
    L2_2 = A0_2.CustomData
    L2_2.nSuppressedCount = (A0_2.CustomData.nSuppressedCount - 1)
  end
  L2_2 = A0_2.CustomData.bActive
  if L2_2 and A1_2 then
    A0_2.Close(A0_2)
  end
end

SetSuppressed = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2, A9_2, A10_2, A11_2, A12_2, A13_2)
  local L14_2, L15_2
  L14_2 = {}
  L14_2.sName = A1_2
  L15_2 = A2_2 or L15_2
  if not A2_2 then
    L15_2 = 0
  end
  L14_2.nX = L15_2
  L15_2 = A3_2 or L15_2
  if not A3_2 then
    L15_2 = 0
  end
  L14_2.nY = L15_2
  L14_2.sLabel = A4_2
  L14_2.sDesc = A5_2
  L14_2.uGuid = A6_2
  L14_2.sTexture = A7_2
  L14_2.sMission = A8_2
  L14_2.nMeter = A9_2
  L14_2.bSticky = A10_2
  L14_2.bTodoList = A11_2
  L14_2.sFaction = A12_2
  L14_2.nSortOrder = A13_2
  L15_2 = A0_2.CustomData.tMapBlips
  L15_2[A1_2] = L14_2
end

AddMapBlip = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  L2_2 = A0_2.CustomData.tMapBlips
  L2_2[A1_2] = nil
  L2_2 = A0_2.CustomData.bActive
  if L2_2 then
    L2_2 = A0_2.CustomData.oMapFlash
    if L2_2 then
      _GuiInternal.RemovePdaBlip(A0_2.CustomData.oMapFlash.BasicData.uId, A1_2)
    end
  end
end

RemoveMapBlip = L0_1
_nMissionCount = 1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2, A9_2)
  local L10_2, L11_2, L12_2, L13_2, L14_2
  L10_2 = A0_2.CustomData.tMissions
  L11_2 = type(A1_2)
  if "string" ~= L11_2 then
    L11_2 = false
    return L11_2
  end
  L11_2 = nil
  L12_2 = L10_2[A1_2]
  if L12_2 then
    L11_2 = L10_2[A1_2]
    L12_2 = A2_2 or L12_2
    if not A2_2 then
      L12_2 = L11_2.sLabel
    end
    L11_2.sLabel = L12_2
    L12_2 = A3_2 or L12_2
    if not A3_2 then
      L12_2 = L11_2.sDesc
    end
    L11_2.sDesc = L12_2
    L12_2 = A4_2 or L12_2
    if not A4_2 then
      L12_2 = L11_2.sFaction
    end
    L11_2.sFaction = L12_2
    L12_2 = A5_2 or L12_2
    if not A5_2 then
      L12_2 = L11_2.sDefaultBlipTexture
    end
    L11_2.sDefaultBlipTexture = L12_2
    L12_2 = A6_2 or L12_2
    if not A6_2 then
      L12_2 = L11_2.sDefaultBlipLabel
    end
    L11_2.sDefaultBlipLabel = L12_2
    L12_2 = A7_2 or L12_2
    if not A7_2 then
      L12_2 = L11_2.bSuppress
    end
    L11_2.bSuppress = L12_2
    L12_2 = A9_2 or L12_2
    if not A9_2 then
      L12_2 = L11_2.nSortOrder
    end
    L11_2.nSortOrder = L12_2
    if A7_2 then
      A8_2 = false
    elseif nil == A8_2 then
      A8_2 = L11_2.bTrackable
    end
    L12_2 = A8_2 or L12_2
    if not A8_2 then
      L12_2 = false
    end
    L11_2.bTrackable = L12_2
    if A7_2 then
      L11_2.sFaction = " "
    end
  else
    if not A7_2 then
      L12_2 = type(A2_2)
      if "string" == L12_2 then
        L12_2 = type(A3_2)
        if "string" == L12_2 then
          L12_2 = type(A4_2)
          if "string" == L12_2 then
            goto lbl_77
          end
        end
      end
      L12_2 = false
      return L12_2
    end
    ::lbl_77::
    if A7_2 then
      A8_2 = false
    elseif nil == A8_2 then
      A8_2 = true
    end
    L12_2 = tostring(_nMissionCount)
    _nMissionCount = (_nMissionCount + 1)
    L13_2 = {}
    L14_2 = A2_2 or L14_2
    if not A2_2 then
      L14_2 = " "
    end
    L13_2.sLabel = L14_2
    L14_2 = A3_2 or L14_2
    if not A3_2 then
      L14_2 = " "
    end
    L13_2.sDesc = L14_2
    L13_2.sFaction = A4_2
    L14_2 = A5_2 or L14_2
    if not A5_2 then
      L14_2 = "icon_yellow_mc"
    end
    L13_2.sDefaultBlipTexture = L14_2
    L14_2 = A6_2 or L14_2
    if not A6_2 then
      L14_2 = "DESIGNER ERROR"
    end
    L13_2.sDefaultBlipLabel = L14_2
    L14_2 = A7_2 or L14_2
    if not A7_2 then
      L14_2 = false
    end
    L13_2.bSuppress = L14_2
    L14_2 = A8_2 or L14_2
    if not A8_2 then
      L14_2 = false
    end
    L13_2.bTrackable = L14_2
    L13_2.sId = L12_2
    L13_2.nSortOrder = A9_2
    L11_2 = L13_2
    if A7_2 then
      L11_2.sFaction = " "
    end
    L10_2[A1_2] = L11_2
    L13_2 = A0_2.CustomData.tMissionIds
    L13_2[L12_2] = A1_2
  end
  L12_2 = true
  return L12_2
end

AddMapMission = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L2_2 = A0_2.CustomData.tMissions
  L3_2 = type(A1_2)
  if "string" ~= L3_2 then
    return
  end
  L3_2 = L2_2[A1_2]
  if L3_2 then
    L5_2 = A0_2.CustomData.tMissionIds
    L5_2[L3_2.sId] = nil
    L3_2 = nil
  end
  L2_2[A1_2] = nil
  L4_2 = {}
  L5_2 = pairs
  L6_2 = A0_2.CustomData.tMapBlips
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = L9_2.sMission
    if A1_2 == L10_2 then
      table.insert(L4_2, L8_2)
    end
  end
  L5_2 = pairs
  L6_2 = L4_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    A0_2.RemoveMapBlip(A0_2, L9_2)
  end
  L5_2 = A0_2.CustomData.sSelectedMission
  if L5_2 == A1_2 then
    SetSelectedMission(A0_2, nil, true)
  end
end

RemoveMapMission = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2)
  local L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  AddMapMission(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, bTrackable, A8_2)
end

UpdateMapMission = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  L3_2 = A0_2.CustomData.tMissions[A1_2]
  if L3_2 then
    L4_2 = A0_2.CustomData.tMissions[A1_2].bSuppress
    if L4_2 then
      A2_2 = false
    end
    L4_2 = A2_2 or L4_2
    if not A2_2 then
      L4_2 = false
    end
    L3_2.bTrackable = L4_2
  end
end

SetMissionTrackable = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2)
  local L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  if A1_2 then
    L7_2 = A0_2.CustomData.tRegions[A1_2]
    if not L7_2 then
      L7_2 = {}
    end
    L8_2 = _Clamp(A2_2, 0, 255)
    L9_2 = _Clamp(A3_2, 0, 255)
    L10_2 = _Clamp(A4_2, 0, 255)
    L11_2 = _Clamp(A5_2, 0, 255)
    if not L8_2 then
      L8_2 = 64
    end
    if not L9_2 then
      L9_2 = 64
    end
    if not L10_2 then
      L10_2 = 160
    end
    if not L11_2 then
      L11_2 = 128
    end
    L11_2 = (L11_2 / 255) * 100
    L7_2.sColor = ("0x" .. string.format("%02X", L8_2) .. string.format("%02X", L9_2) .. string.format("%02X", L10_2))
    L7_2.nAlpha = L11_2
    L7_2.bInvert = A6_2
    L12_2 = A0_2.CustomData.tRegions
    L12_2[A1_2] = L7_2
  end
end

AddLineRegion = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  if A0_2 then
    L3_2 = type(A0_2)
    if "number" == L3_2 then
      goto lbl_9
    end
  end
  do return end
  ::lbl_9::
  if A2_2 < A0_2 then
    return A2_2
  end
  if A0_2 < A1_2 then
    return A1_2
  end
  return A0_2
end

_Clamp = L0_1

function L0_1(A0_2, A1_2)
  local L2_2
  if A1_2 then
    L2_2 = A0_2.CustomData.tRegions
    L2_2[A1_2] = nil
  end
end

RemoveLineRegion = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L3_2 = Net.IsClient()
  if L3_2 and A2_2 ~= true then
    return
  end
  if A1_2 then
    L3_2 = A0_2.CustomData.tMissions[A1_2]
    if L3_2 then
      L3_2 = A0_2.CustomData
      L3_2.sSelectedMission = A1_2
  end
  else
    L3_2 = A0_2.CustomData
    L3_2.sSelectedMission = nil
  end
  L3_2 = Net.IsServer()
  if L3_2 then
    L6_2 = {}
    L7_2 = WifMissionData.GetMissionIndexFromId
    L8_2 = A0_2.CustomData.sSelectedMission
    L7_2, L8_2 = L7_2(L8_2)
    L6_2[1] = L7_2
    L6_2[2] = L8_2
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_SETSELECTEDMISSION, L6_2)
  end
end

SetSelectedMission = L0_1

function L0_1(A0_2)
  local L1_2
  L1_2 = A0_2.CustomData.sSelectedMission
  return L1_2
end

GetSelectedMission = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  L3_2 = type(A1_2)
  if "function" == L3_2 or nil == A1_2 then
    L3_2 = A0_2.CustomData
    L3_2.fMissionChangeCallback = A1_2
    L3_2 = A0_2.CustomData
    L3_2.tMissionChangeData = A2_2
  end
end

SetMissionTrackCallback = L0_1

function L0_1(A0_2, A1_2)
  local L2_2
  L2_2 = Net.IsClient()
  if L2_2 then
    L2_2 = A0_2.CustomData
    L2_2.bAllowTrackingChange = false
  else
    L2_2 = A0_2.CustomData
    L2_2.bAllowTrackingChange = A1_2
  end
end

SetMissionChangeAllowed = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2
  if nil == A1_2 then
    L4_2 = A0_2.CustomData
    L4_2.nFakePlayerX = nil
    L4_2 = A0_2.CustomData
    L4_2.nFakePlayerY = nil
    L4_2 = A0_2.CustomData
    L4_2.nFakePlayerZ = nil
    return
  end
  L4_2 = type(A1_2)
  if "number" == L4_2 then
    L4_2 = type(A2_2)
    if "number" == L4_2 then
      L4_2 = type(A3_2)
      if "number" == L4_2 then
        L4_2 = A0_2.CustomData
        L4_2.nFakePlayerX = A1_2
        L4_2 = A0_2.CustomData
        L4_2.nFakePlayerY = A2_2
        L4_2 = A0_2.CustomData
        L4_2.nFakePlayerZ = A3_2
      end
    end
  end
end

SetFakePlayerLocation = L0_1

function L0_1(A0_2, A1_2)
  local L2_2
  L2_2 = A0_2.CustomData
  L2_2.bBeaconTutorialMode = A1_2
end

SetBeaconTutorialMode = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2
  if nil == A2_2 then
    A2_2 = true
  end
  L5_2 = 0
  L8_2 = A0_2
  L7_2 = A0_2.GetOwner
  L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2 = L7_2(L8_2)
  L6_2 = Player.GetCamera(L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2)
  if L6_2 then
    L5_2 = Camera.GetYaw(L6_2)
  end
  if not A3_2 then
    A3_2 = _knBlipLimit * 0.5
  end
  if A3_2 < 0 then
    A3_2 = _knBlipLimit * 0.5
  end
  L7_2 = 35
  L8_2 = 40
  L9_2 = 1
  L10_2 = Gui.GetMapCorrectionOffset
  if L10_2 then
    L10_2 = Gui.GetMapCorrectionOffset
    L10_2, L11_2, L12_2 = L10_2()
    L9_2 = L12_2
    L8_2 = L11_2
    L7_2 = L10_2
  end
  L10_2 = 1
  L11_2 = nil
  L12_2 = nil
  L13_2 = " "
  L14_2 = A0_2.CustomData.sSelectedMission
  if L14_2 then
    L11_2 = A0_2.CustomData.tMissions[A0_2.CustomData.sSelectedMission]
    if L11_2 then
      L14_2 = L11_2.bSuppress
      if not L14_2 then
        L12_2 = L11_2.sDesc
        L13_2 = "[PDA.Map.CurrentMission]"
      end
    end
  end
  L14_2 = A0_2.GetOwner(A0_2)
  L15_2 = A0_2.CustomData.oMapFlash.BasicData.uId
  L14_2 = Player.GetCharacter(L14_2)
  L16_2 = nil
  L17_2 = nil
  L18_2 = nil
  L19_2 = A0_2.CustomData.nFakePlayerX
  if L19_2 then
    L16_2 = A0_2.CustomData.nFakePlayerX
    L17_2 = A0_2.CustomData.nFakePlayerY
    L18_2 = A0_2.CustomData.nFakePlayerZ
  else
    L19_2 = Object.GetPosition
    L20_2 = L14_2
    L19_2, L20_2, L21_2 = L19_2(L20_2)
    L18_2 = L21_2
    L17_2 = L20_2
    L16_2 = L19_2
  end
  if not A1_2 then
    A1_2 = A0_2.CustomData.oMapFlash
  end
  L19_2 = {}
  L20_2 = nil
  L21_2 = string.format("[PDA.Map.Player:%d]", 1)
  L22_2 = {}
  L23_2 = "player1_mc"
  L24_2 = "player1_mc"
  L25_2 = (L16_2 + L7_2) * L9_2
  L26_2 = (L18_2 + L8_2) * L9_2
  L27_2 = math.rad(L5_2)
  L28_2 = L21_2
  L29_2 = L12_2 or L29_2
  if not L12_2 then
    L29_2 = L21_2
  end
  L30_2 = " "
  L22_2[1] = L23_2
  L22_2[2] = L24_2
  L22_2[3] = L25_2
  L22_2[4] = L26_2
  L22_2[5] = L27_2
  L22_2[6] = L28_2
  L22_2[7] = L29_2
  L22_2[8] = L30_2
  L22_2[9] = " "
  L22_2[10] = false
  L22_2[11] = "  "
  L22_2[12] = L13_2
  L22_2[13] = true
  L22_2[14] = false
  L22_2[15] = false
  table.insert(L19_2, L22_2)
  L22_2 = {}
  L22_2.bNoMission = true
  L22_2.sId = ""
  L22_2.sLabel = ""
  L23_2 = {}
  L23_2.bNoMission = true
  L23_2.sId = "m"
  L24_2 = false
  L25_2 = false
  L26_2 = nil
  L27_2 = {}
  L28_2 = pairs
  L29_2 = A0_2.CustomData.tMissions
  L28_2, L29_2, L30_2 = L28_2(L29_2)
  for L31_2, L32_2 in L28_2, L29_2, L30_2 do
    L33_2 = L32_2.bSuppress
    if not L33_2 then
      table.insert(L27_2, L32_2)
    end
  end
  L30_2 = _MissionSortLessThan
  table.sort(L27_2, L30_2)
  L28_2 = pairs
  L29_2 = L27_2
  L28_2, L29_2, L30_2 = L28_2(L29_2)
  for L31_2, L32_2 in L28_2, L29_2, L30_2 do
    L26_2 = _tFactionNameLookup[L32_2.sFaction]
    L33_2 = {}
    L33_2[1] = L32_2.sLabel
    L33_2[2] = L32_2.sDefaultBlipTexture
    L33_2[3] = 10000
    L33_2[4] = 10000
    L33_2[5] = 0
    L33_2[6] = L32_2.sLabel
    L33_2[7] = L32_2.sDesc
    L33_2[8] = L32_2.sFaction
    L33_2[9] = L26_2
    L33_2[10] = true
    L33_2[11] = L32_2.sId
    L33_2[12] = L32_2.sLabel
    L33_2[13] = false
    L33_2[14] = false
    L20_2 = L33_2
    if not A4_2 then
      table.insert(L19_2, L20_2)
    end
  end
  L28_2 = false
  L31_2 = {}
  L32_2 = 5
  L33_2 = 1
  L34_2 = -1
  for L35_2 = L32_2, L33_2, L34_2 do
    L31_2[L35_2] = {}
  end
  L32_2 = pairs
  L33_2 = A0_2.CustomData.tMapBlips
  L32_2, L33_2, L34_2 = L32_2(L33_2)
  for L35_2, L36_2 in L32_2, L33_2, L34_2 do
    L37_2 = L36_2.nSortOrder
    if L37_2 then
      L37_2 = L31_2[L36_2.nSortOrder]
      if L37_2 then
        table.insert(L31_2[L36_2.nSortOrder], L36_2)
    end
    else
      table.insert(L31_2[L29_2], L36_2)
    end
  end
  L32_2 = {}
  L33_2 = L29_2
  L34_2 = L30_2
  L35_2 = -1
  for L36_2 = L33_2, L34_2, L35_2 do
    L37_2 = pairs
    L38_2 = L31_2[L36_2]
    L37_2, L38_2, L39_2 = L37_2(L38_2)
    for L40_2, L41_2 in L37_2, L38_2, L39_2 do
      table.insert(L32_2, L41_2)
    end
  end
  L33_2 = 0
  L34_2 = 1
  L35_2 = #L32_2
  if A3_2 < L35_2 then
    L34_2 = #L32_2 - (A3_2 + 1)
  end
  while true do
    L35_2 = L32_2[L34_2]
    if not L35_2 then
      break
    end
    L35_2 = L32_2[L34_2]
    L22_2.sFaction = nil
    L26_2 = nil
    L36_2 = L35_2.uGuid
    if L36_2 then
      L36_2 = Object.GetPosition
      L37_2 = L35_2.uGuid
      L36_2, L37_2, L38_2 = L36_2(L37_2)
      if L36_2 and L38_2 then
        L35_2.nX = L36_2
        L35_2.nY = L38_2
      end
    end
    L36_2 = nil
    L24_2 = L35_2.bSticky or L24_2
    if not L37_2 then
      L24_2 = false
    end
    L25_2 = false
    L37_2 = L35_2.sMission
    if L37_2 then
      L36_2 = A0_2.CustomData.tMissions[L35_2.sMission] or L36_2
      if not L37_2 then
        L36_2 = L22_2
      end
      L37_2 = L35_2.bSticky
      if nil ~= L37_2 then
        L24_2 = L35_2.bSticky
      else
        L37_2 = A0_2.CustomData.sSelectedMission
        if L37_2 then
          L37_2 = A0_2.CustomData.sSelectedMission
          L38_2 = L35_2.sMission
          if L37_2 == L38_2 then
            L24_2 = true
          end
        end
      end
      L37_2 = A0_2.CustomData.bAllowTrackingChange
      if L37_2 then
        L25_2 = L36_2.bTrackable
      end
      L37_2 = L36_2.bSuppress
      if L37_2 then
        L36_2 = L22_2
      end
    else
      L37_2 = L35_2.bTodoList
      if L37_2 then
        L36_2 = L23_2
        L23_2.sFaction = L35_2.sFaction
        L23_2.sId = L35_2.sName
        L23_2.sLabel = L35_2.sLabel
      else
        L36_2 = L22_2
      end
    end
    L37_2 = L36_2.sFaction
    if L37_2 then
      L26_2 = _tFactionNameLookup[L36_2.sFaction]
    else
      L37_2 = L35_2.sFaction
      if L37_2 then
        L26_2 = _tFactionNameLookup[L35_2.sFaction]
      end
    end
    L28_2 = false
    L37_2 = L36_2.bNoMission
    if not L37_2 then
      L37_2 = L36_2.bTrackable
      if L37_2 then
        L28_2 = true
      end
    end
    L37_2 = {}
    L38_2 = L35_2.sName
    L39_2 = L35_2.sTexture
    if not L39_2 then
      L39_2 = L36_2.sDefaultBlipTexture
      if not L39_2 then
        L39_2 = "icon_yellow_mc"
      end
    end
    L40_2 = (L35_2.nX + L7_2) * L9_2
    L41_2 = (L35_2.nY + L8_2) * L9_2
    L42_2 = 0
    L43_2 = L35_2.sLabel
    if not L43_2 then
      L43_2 = L36_2.sDefaultBlipLabel
    end
    L44_2 = L35_2.sDesc
    if not L44_2 then
      L44_2 = L36_2.sDesc
    end
    L45_2 = L36_2.sFaction
    if not L45_2 then
      L45_2 = L35_2.sFaction
    end
    L37_2[1] = L38_2
    L37_2[2] = L39_2
    L37_2[3] = L40_2
    L37_2[4] = L41_2
    L37_2[5] = L42_2
    L37_2[6] = L43_2
    L37_2[7] = L44_2
    L37_2[8] = L45_2
    L37_2[9] = L26_2
    L37_2[10] = L28_2
    L37_2[11] = L36_2.sId
    L37_2[12] = L36_2.sLabel
    L37_2[13] = L24_2
    L37_2[14] = L25_2
    L37_2[15] = L24_2
    L37_2 = L37_2[2]
    if A4_2 then
      if "icon_action_3_mc" ~= L37_2 and "icon_outpost_3_mc" ~= L37_2 and "icon_defend_3_mc" ~= L37_2 and "icon_destroy_3_mc" ~= L37_2 and "icon_verify_3_mc" ~= L37_2 and "icon_deliverable_3_mc" ~= L37_2 then
        table.insert(L19_2, L20_2)
      end
    else
      table.insert(L19_2, L20_2)
    end
    L34_2 = L34_2 + 1
  end
  L35_2 = A0_2.CustomData.nMarkerX
  if L35_2 then
    L35_2 = A0_2.CustomData.nMarkerZ
    if L35_2 then
      L35_2 = {}
      L35_2[1] = "marker_mc"
      L35_2[2] = "marker_mc"
      L35_2[3] = A0_2.CustomData.nMarkerX
      L35_2[4] = A0_2.CustomData.nMarkerZ
      L35_2[5] = 0
      L35_2[6] = "MARKER"
      L35_2[7] = "Destination Marker"
      L35_2[8] = "  "
      L35_2[9] = "  "
      L35_2[10] = false
      L35_2[11] = "  "
      L35_2[12] = "  "
      L35_2[13] = true
      L35_2[14] = false
      L35_2[15] = false
      table.insert(L19_2, L35_2)
  end
  else
    L35_2 = {}
    L35_2[1] = "marker_mc"
    L35_2[2] = "marker_mc"
    L35_2[3] = L16_2
    L35_2[4] = L17_2
    L35_2[5] = 0
    L35_2[6] = "MARKER"
    L35_2[7] = "Destination Marker"
    L35_2[8] = "  "
    L35_2[9] = "  "
    L35_2[10] = false
    L35_2[11] = "  "
    L35_2[12] = "  "
    L35_2[13] = true
    L35_2[14] = false
    L35_2[15] = false
    table.insert(L19_2, L35_2)
  end
  _GuiInternal.AddPdaMapBlips(A1_2.BasicData.uId, L19_2)
  AddPDATargetMarkers(A0_2)
  L35_2 = A0_2.CustomData.nMarkerX
  if L35_2 then
    L35_2 = A0_2.CustomData.nMarkerZ
    if L35_2 then
      L38_2 = {}
      L38_2[1] = true
      A1_2.CallActionScriptCallback(A1_2, "SetMarker", L38_2)
  end
  else
    L38_2 = {}
    L38_2[1] = false
    A1_2.CallActionScriptCallback(A1_2, "SetMarker", L38_2)
  end
  UpdateAllPlayerMarkers(A0_2)
  _DisplayRegions(A0_2, L7_2, L8_2, L9_2)
  L35_2 = nil
  L36_2 = A0_2.CustomData.sSelectedMission
  if L36_2 then
    L36_2 = A0_2.CustomData.bAllowTrackingChange
    if not L36_2 then
      L36_2 = A0_2.CustomData.tMissions[A0_2.CustomData.sSelectedMission]
      if L36_2 then
        L35_2 = L36_2.sId
      end
    end
  end
  L40_2 = {}
  L40_2[1] = A1_2
  A1_2.SetFlashEventHandler(A1_2, "beaconCheck", HandleBeaconCheck, L40_2)
  L36_2 = A0_2.CustomData.bBeaconTutorialMode
  if L36_2 then
    A1_2.CallActionScriptCallback(A1_2, "beaconTutorial", {})
  end
  if A2_2 then
    L39_2 = {}
    L39_2[1] = false
    A1_2.CallActionScriptCallback(A1_2, "LandingZone", L39_2)
    if L35_2 then
      L39_2 = {}
      L39_2[1] = L35_2
      A1_2.CallActionScriptCallback(A1_2, "activeContract", L39_2)
    end
    A1_2.CallActionScriptCallback(A1_2, "AddBlipFinish", {})
  end
end

_PopulateMapDisplay = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = A0_2.nSortOrder
  if not L2_2 then
    L2_2 = false
    return L2_2
  else
    L2_2 = A1_2.nSortOrder
    if not L2_2 then
      L2_2 = true
      return L2_2
    end
  end
  L2_2 = A0_2.nSortOrder < A1_2.nSortOrder
  return L2_2
end

_MissionSortLessThan = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L4_2 = {}
  L5_2 = 1
  L6_2 = string.gmatch
  L7_2 = A1_2
  L8_2 = "-*%d+"
  L6_2, L7_2, L8_2 = L6_2(L7_2, L8_2)
  for L9_2 in L6_2, L7_2, L8_2 do
    L4_2[L5_2] = tonumber(L9_2)
    L5_2 = L5_2 + 1
  end
  L6_2 = L4_2[1]
  if L6_2 then
    L6_2 = L4_2[2]
    if L6_2 then
      L2_2 = L4_2[1] * 2
      L3_2 = L4_2[2] * 2
    end
  end
  L6_2 = type(L2_2)
  if "number" == L6_2 then
    L6_2 = type(L3_2)
    if "number" == L6_2 then
      L6_2 = Player.IsPositionOutBoundary
      if L6_2 then
        L6_2 = 35
        L7_2 = 40
        L8_2 = 1
        L9_2 = Gui.GetMapCorrectionOffset
        if L9_2 then
          L9_2 = Gui.GetMapCorrectionOffset
          L9_2, L10_2, L11_2 = L9_2()
          L8_2 = L11_2
          L7_2 = L10_2
          L6_2 = L9_2
        end
        L9_2 = Player.IsPositionOutBoundary(Player.GetLocalPlayer(), (((L2_2 * 0.5) / L8_2) - L6_2), 0, (((L3_2 * 0.5) / L8_2) - L7_2))
        if not L9_2 then
          L12_2 = {}
          L12_2[1] = true
          A0_2.CallActionScriptCallback(A0_2, "beaconCheckReturn", L12_2)
          return
        end
      end
    end
  end
  L9_2 = {}
  L9_2[1] = false
  A0_2.CallActionScriptCallback(A0_2, "beaconCheckReturn", L9_2)
end

HandleBeaconCheck = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  if not A1_2 then
    A1_2 = 0
  end
  if not A2_2 then
    A2_2 = 0
  end
  L4_2 = A0_2.CustomData.oMapFlash
  L5_2 = 1
  L6_2 = pairs
  L7_2 = A0_2.CustomData.tRegions
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    _DisplayRegion(L4_2, L9_2, L5_2, L10_2.sColor, L10_2.nAlpha, A1_2, A2_2, L10_2.bInvert, A3_2)
    L5_2 = L5_2 + 1
  end
end

_DisplayRegions = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2)
  local L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2
  L9_2 = Pg.GetLineRegionPoints
  L10_2 = A1_2
  L9_2, L10_2 = L9_2(L10_2, A7_2)
  L11_2 = nil
  L12_2 = true
  L13_2 = ipairs
  L14_2 = L9_2
  L13_2, L14_2, L15_2 = L13_2(L14_2)
  for L16_2, L17_2 in L13_2, L14_2, L15_2 do
    L11_2 = L10_2[L16_2]
    if L12_2 then
      L21_2 = {}
      L25_2 = (L17_2 + A5_2) * A8_2
      L26_2 = (L11_2 + A6_2) * A8_2
      L21_2[1] = A2_2
      L21_2[2] = true
      L21_2[3] = false
      L21_2[4] = L25_2
      L21_2[5] = L26_2
      L21_2[6] = A3_2
      L21_2[7] = A4_2
      A0_2.CallActionScriptCallback(A0_2, "AddZone", L21_2)
      L12_2 = false
    else
      L21_2 = {}
      L25_2 = (L17_2 + A5_2) * A8_2
      L26_2 = (L11_2 + A6_2) * A8_2
      L21_2[1] = A2_2
      L21_2[2] = false
      L21_2[3] = false
      L21_2[4] = L25_2
      L21_2[5] = L26_2
      A0_2.CallActionScriptCallback(A0_2, "AddZone", L21_2)
    end
  end
  L16_2 = {}
  L16_2[1] = A2_2
  L16_2[2] = false
  L16_2[3] = true
  A0_2.CallActionScriptCallback(A0_2, "AddZone", L16_2)
end

_DisplayRegion = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = A0_2.oParentWidget
  L2_2.SetSelectedMission(L2_2, L2_2.CustomData.tMissionIds[A1_2])
  L4_2 = L2_2.CustomData.fMissionChangeCallback
  if L4_2 then
    L4_2 = {}
    L5_2 = L2_2.CustomData.tMissionChangeData
    if L5_2 then
      L5_2 = ipairs
      L6_2 = L2_2.CustomData.tMissionChangeData
      L5_2, L6_2, L7_2 = L5_2(L6_2)
      for L8_2, L9_2 in L5_2, L6_2, L7_2 do
        L4_2[L8_2] = L9_2
      end
    end
    table.insert(L4_2, L3_2)
    L6_2 = unpack
    L7_2 = L4_2
    L6_2, L7_2, L8_2, L9_2, L10_2 = L6_2(L7_2)
    L2_2.CustomData.fMissionChangeCallback(L6_2, L7_2, L8_2, L9_2, L10_2)
  end
end

_HandleTrackEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = A0_2.oParentWidget
  L2_2.SetSelectedMission(L2_2, nil)
  L3_2 = L2_2.CustomData.fMissionChangeCallback
  if L3_2 then
    L3_2 = L2_2.CustomData.tMissionChangeData
    if not L3_2 then
      L3_2 = {}
    end
    L5_2 = unpack
    L6_2 = L3_2
    L5_2, L6_2 = L5_2(L6_2)
    L2_2.CustomData.fMissionChangeCallback(L5_2, L6_2)
  end
end

_HandleUntrackEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  L2_2 = MrxPlayState.GetCurrentMission()
  if L2_2 then
    L2_2.Cancel(L2_2)
  end
end

_HandleMissionCancel = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2.CustomData
  L2_2.nMarkerX = A1_2.PosX
  L2_2 = A0_2.CustomData
  L2_2.nMarkerZ = A1_2.PosZ
  L4_2 = {}
  L4_2.nX = A1_2.PosX
  L4_2.nY = A1_2.PosZ
  Event.Post("GPS Beacon Set", L4_2)
end

HandleMarkerUpdate = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2.CustomData
  L2_2.nMarkerX = nil
  L2_2 = A0_2.CustomData
  L2_2.nMarkerZ = nil
  L4_2 = {}
  L4_2.nX = A1_2.PosX
  L4_2.nY = A1_2.PosZ
  Event.Post("GPS Beacon Cleared", L4_2)
end

HandleMarkerClear = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L4_2 = A0_2.CustomData.bActive
  if L4_2 then
    return
  end
  L4_2 = A0_2.CustomData
  L4_2.bActive = true
  L4_2 = A0_2.CustomData.oMapFlash
  L5_2 = L4_2
  L4_2 = L4_2.GetLocation
  L4_2, L5_2, L6_2, L7_2 = L4_2(L5_2)
  L8_2 = A0_2.CustomData.oTransit
  if not L8_2 then
    L9_2 = MrxGuiBase.FlashWidget
    L8_2 = L9_2.new(L9_2)
    L13_2 = L6_2
    L14_2 = L7_2
    L8_2.SetLocation(L8_2, L4_2, L5_2, L13_2, L14_2)
    L12_2 = A0_2
    L11_2 = A0_2.GetOwner
    L11_2, L12_2, L13_2, L14_2, L15_2, L16_2 = L11_2(L12_2)
    L8_2.SetOwner(L8_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2)
    L9_2 = A0_2.CustomData
    L9_2.oTransit = L8_2
    A0_2.AddChild(A0_2, L8_2)
    L8_2.oParentWidget = A0_2
  end
  L9_2 = L8_2.CustomData
  L9_2.fCallback = A2_2
  L9_2 = L8_2.CustomData
  L9_2.tCallbackData = A3_2
  L13_2 = {}
  L13_2[1] = L8_2
  L13_2[2] = A0_2
  L13_2[3] = A1_2
  L8_2.SetSwfFile(L8_2, "landingzones", _FinishTransitInterfaceLoad, L13_2)
  _SetupDelayedOpenSound(0.6, A0_2)
  MrxSound.EnterPDAState()
  L11_2 = {}
  L11_2.uPlayer = A0_2.GetOwner(A0_2)
  Event.Post("Transit Interface Open", L11_2)
  Sys.RequestGameState("PDA")
end

OpenTransitInterface = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2
  MrxGuiBase.GetControlFocus(A1_2, true)
  A0_2.Restart(A0_2)
  A0_2.Play(A0_2)
  _PopulateMapDisplay(A1_2, A0_2, false, (_knBlipLimit - #A2_2), true)
  L3_2 = 35
  L4_2 = 40
  L5_2 = 1
  L6_2 = Gui.GetMapCorrectionOffset
  if L6_2 then
    L6_2 = Gui.GetMapCorrectionOffset
    L6_2, L7_2, L8_2 = L6_2()
    L5_2 = L8_2
    L4_2 = L7_2
    L3_2 = L6_2
  end
  L6_2 = A0_2
  L10_2 = {}
  L10_2[1] = 1
  L6_2.CallActionScriptCallback(L6_2, "LandingZone", L10_2)
  L6_2.SetFlashEventHandler(L6_2, "LandingZone", _InvokeCallbackSuccess, {})
  L10_2 = _HandleCloseEvent
  L6_2.SetFlashEventHandler(L6_2, "closeMap", L10_2, {})
  L7_2 = {}
  L8_2 = pairs
  L9_2 = A2_2
  L8_2, L9_2, L10_2 = L8_2(L9_2)
  for L11_2, L12_2 in L8_2, L9_2, L10_2 do
    L12_2.nId = L11_2
    table.insert(L7_2, L12_2)
  end
  L10_2 = _LandingZoneLessThan
  table.sort(L7_2, L10_2)
  L8_2 = pairs
  L9_2 = L7_2
  L8_2, L9_2, L10_2 = L8_2(L9_2)
  for L11_2, L12_2 in L8_2, L9_2, L10_2 do
    L14_2 = L6_2
    L13_2 = L6_2.CallActionScriptCallback
    L15_2 = "AddBlip"
    L16_2 = {}
    L17_2 = tostring(L12_2.nId)
    L18_2 = "icon_lz_mc"
    L19_2 = (L12_2.nX + L3_2) * L5_2
    L20_2 = (L12_2.nY + L4_2) * L5_2
    L21_2 = 0
    L22_2 = L12_2.sName
    if not L22_2 then
      L22_2 = "Needs localized name"
    end
    L16_2[1] = L17_2
    L16_2[2] = L18_2
    L16_2[3] = L19_2
    L16_2[4] = L20_2
    L16_2[5] = L21_2
    L16_2[6] = L22_2
    L16_2[7] = "Landing Zone"
    L16_2[8] = " "
    L16_2[9] = " "
    L16_2[10] = false
    L16_2[11] = " "
    L16_2[12] = " "
    L13_2(L14_2, L15_2, L16_2)
  end
  A1_2.SetVisible(A1_2, true)
  L8_2 = A1_2.GetChildren(A1_2)
  L9_2 = ipairs
  L10_2 = L8_2
  L9_2, L10_2, L11_2 = L9_2(L10_2)
  for L12_2, L13_2 in L9_2, L10_2, L11_2 do
    L14_2 = A1_2.CustomData.oMapFlash
    if L13_2 ~= L14_2 then
      L17_2 = A1_2
      L16_2 = A1_2.GetOwner
      L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2 = L16_2(L17_2)
      L13_2.SetOwner(L13_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2)
      MrxGuiBase.AddWidgetWithChildren(L13_2)
    else
      MrxGuiBase.RemoveWidgetWithChildren(L13_2)
    end
  end
  L11_2 = A1_2
  L10_2 = A1_2.GetOwner
  L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2 = L10_2(L11_2)
  L9_2 = MrxGuiManager.GetHudState(L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2)
  L10_2 = A1_2.CustomData
  L10_2.bHudState = L9_2
  if L9_2 then
    MrxGuiManager.ToggleHud(A1_2.GetOwner(A1_2), false)
  end
  L6_2.CallActionScriptCallback(L6_2, "AddBlipFinish", {})
end

_FinishTransitInterfaceLoad = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = A0_2.nSortOrder
  if not L2_2 then
    L2_2 = false
    return L2_2
  else
    L2_2 = A1_2.nSortOrder
    if not L2_2 then
      L2_2 = true
      return L2_2
    end
  end
  L2_2 = A0_2.nSortOrder < A1_2.nSortOrder
  return L2_2
end

_LandingZoneLessThan = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = A0_2.CustomData.oTransit
  Sys.RequestGameState("ingame")
  if L1_2 then
    L5_2 = {}
    L5_2[1] = true
    L1_2.CallActionScriptCallback(L1_2, "requestClose", L5_2)
    A0_2.RemoveChild(A0_2, L1_2)
    L2_2 = A0_2.CustomData
    L2_2.oTransit = nil
    L4_2 = {}
    L4_2[1] = 0.1
    L4_2[2] = true
    L6_2 = {}
    L6_2[1] = L1_2
    Event.Create(Event.TimerRelative, L4_2, _RemoveTransitInterfaceDelayed, L6_2)
    L2_2 = L1_2.CustomData.fCallback
    if L2_2 then
      _InvokeCallback(L1_2, "0", false)
    end
  end
end

_RemoveTransitInterface = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2.SetSwfFile(A0_2, nil)
  A0_2.delete(A0_2)
end

_RemoveTransitInterfaceDelayed = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L4_2 = {}
  L4_2.uPlayer = A0_2.GetOwner(A0_2)
  Event.Post("Transit Interface Success", L4_2)
  _InvokeCallback(A0_2, A1_2, true)
end

_InvokeCallbackSuccess = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = A0_2.CustomData.fCallback
  L4_2 = A0_2.CustomData.tCallbackData
  L5_2 = A0_2.CustomData
  L5_2.fCallback = nil
  L5_2 = A0_2.CustomData
  L5_2.tCallbackData = nil
  if L3_2 then
    if not L4_2 then
      L4_2 = {}
    end
    table.insert(L4_2, 1, A2_2)
    L8_2 = tonumber
    L9_2 = A1_2
    L8_2, L9_2 = L8_2(L9_2)
    table.insert(L4_2, 1, L8_2, L9_2)
    L6_2 = unpack
    L7_2 = L4_2
    L6_2, L7_2, L8_2, L9_2 = L6_2(L7_2)
    L3_2(L6_2, L7_2, L8_2, L9_2)
  end
end

_InvokeCallback = L0_1
nSupportId = 1
L0_1 = {}
L0_1.Airstrike = "AddSupportAirstrike"
L0_1.Civilian = "AddSupportCivilian"
L0_1.Light = "AddSupportLight"
L0_1.Heavy = "AddSupportHeavy"
L0_1.Heli = "AddSupportHelicopters"
L0_1.Boat = "AddSupportBoats"
L0_1.Supply = "AddSupportSupplies"

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = A0_2.CustomData.tSupport[sName]
  if L3_2 then
    L3_2 = UpdateSupport
    L4_2 = A0_2
    L5_2 = A1_2
    return L3_2(L4_2, L5_2)
  end
  L3_2 = {}
  L4_2 = pairs
  L5_2 = A1_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L3_2[L7_2] = L8_2
  end
  L3_2.sId = ("s" .. nSupportId)
  L3_2.sKey = A2_2
  nSupportId = (nSupportId + 1)
  L3_2.sAddFunc = L0_1[L3_2.sType]
  L4_2 = A0_2.CustomData.tSupport
  L4_2[A1_2.sName] = L3_2
  table.insert(A0_2.CustomData.tSupportOrdered, A1_2.sName)
  L4_2 = A0_2.CustomData.tSupportIdIndex
  L4_2[L3_2.sId] = A1_2.sName
end

AddSupport = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = A0_2.CustomData.tSupport[A1_2]
  if L2_2 then
    L2_2 = A0_2.CustomData.tSupport[A1_2].sId
    L3_2 = A0_2.CustomData.tSupportIdIndex
    L3_2[L2_2] = nil
  end
  L2_2 = A0_2.CustomData.tSupport
  L2_2[A1_2] = nil
  L2_2 = 1
  while true do
    L3_2 = A0_2.CustomData.tSupportOrdered[L2_2]
    if not L3_2 then
      break
    end
    L3_2 = A0_2.CustomData.tSupportOrdered[L2_2]
    if L3_2 == A1_2 then
      break
    end
    L2_2 = L2_2 + 1
  end
  L3_2 = A0_2.CustomData.tSupportOrdered
  L3_2[L2_2] = nil
  L3_2 = pairs
  L4_2 = A0_2.CustomData.tEquippedSupport
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    if A1_2 == L7_2 then
      L8_2 = A0_2.CustomData.tEquippedSupport
      L8_2[L6_2] = nil
      L8_2 = A0_2.CustomData.tEquippedSupportIcons
      L8_2[L6_2] = nil
      L11_2 = A0_2
      L10_2 = A0_2.GetOwner
      L10_2, L11_2 = L10_2(L11_2)
      L8_2 = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", L10_2, L11_2)
      if L8_2 then
        L8_2.RemoveItem(L8_2, A1_2)
      end
    end
  end
end

RemoveSupport = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = A0_2.CustomData.tSupport[A1_2.sName]
  if not L2_2 then
    L3_2 = false
    return L3_2
  end
  L3_2 = pairs
  L4_2 = A1_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L8_2 = L7_2 or L8_2
    if not L7_2 then
      L8_2 = L2_2[L6_2]
    end
    L2_2[L6_2] = L8_2
  end
  L2_2.sAddFunc = L0_1[L2_2.sType]
  L3_2 = true
  return L3_2
end

UpdateSupport = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = MrxPmc.GetSupportQty
  L3_2 = A1_2
  return L2_2(L3_2)
end

GetStockpile = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2
  L1_2 = A0_2.CustomData.oMapFlash
  L2_2 = {}
  L2_2.Airstrike = "[airstrike] "
  L2_2.Civilian = "[vehcivilian] "
  L2_2.Light = "[vehmlight] "
  L2_2.Heavy = "[vehmheavy] "
  L2_2.Heli = "[vehheli] "
  L2_2.Boat = "[vehboat] "
  L2_2.Supply = "[supply] "
  L6_2 = {}
  L7_2 = MrxPmc.GetCashQty()
  L8_2 = MrxPmc.GetFuelQty()
  L9_2 = MrxPmc.GetFuelCapacity
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2 = L9_2()
  L6_2[1] = L7_2
  L6_2[2] = L8_2
  L6_2[3] = L9_2
  L6_2[4] = L10_2
  L6_2[5] = L11_2
  L6_2[6] = L12_2
  L6_2[7] = L13_2
  L6_2[8] = L14_2
  L6_2[9] = L15_2
  L6_2[10] = L16_2
  L6_2[11] = L17_2
  L6_2[12] = L18_2
  L6_2[13] = L19_2
  L6_2[14] = L20_2
  L6_2[15] = L21_2
  L6_2[16] = L22_2
  L6_2[17] = L23_2
  L6_2[18] = L24_2
  L6_2[19] = L25_2
  L6_2[20] = L26_2
  L1_2.CallActionScriptCallback(L1_2, "AddStockpile", L6_2)
  _UpdateSupportData(A0_2)
  L3_2 = nil
  L4_2 = nil
  L5_2 = nil
  L6_2 = nil
  L7_2 = pairs
  L8_2 = A0_2.CustomData.tSupportOrdered
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  for L10_2, L11_2 in L7_2, L8_2, L9_2 do
    L3_2 = A0_2.CustomData.tSupport[L11_2]
    L13_2 = L3_2.oSupport
    L14_2 = L13_2
    L13_2 = L13_2.GetSupportName
    L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2 = L13_2(L14_2)
    L4_2 = MrxPmc.GetSupportQty(L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2)
    L6_2 = " "
    L12_2 = L3_2.oSupport
    L12_2 = L12_2.GetDesignator(L12_2)
    if L12_2 then
      L12_2 = L3_2.oSupport
      L12_2 = L12_2.GetDesignator(L12_2)
      L12_2 = L12_2.GetType(L12_2)
      if "smoke" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.Smoke]"
      elseif "satellite" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.Satellite]"
      elseif "advanced satellite" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.AdvSatellite]"
      elseif "beacon" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.Beacon]"
      elseif "laser" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.Laser]"
      elseif "flare" == L12_2 then
        L6_2 = "[Generic.SupportDesignators.Flare]"
      end
    end
    L5_2 = MrxSupportData.IsSupportEquippable(L3_2.sKey)
    L12_2 = L3_2.sAddFunc
    if L12_2 and L4_2 and 0 < L4_2 then
      L12_2 = MrxPmc.IsSupportNew(L3_2.sKey)
      L14_2 = L1_2
      L13_2 = L1_2.CallActionScriptCallback
      L15_2 = L3_2.sAddFunc
      L16_2 = {}
      L17_2 = L3_2.sId
      L18_2 = L2_2[L3_2.sType]
      if not L18_2 then
        L18_2 = ""
      end
      L16_2[1] = L17_2
      L16_2[2] = (L18_2 .. L3_2.sName)
      L16_2[3] = L3_2.sDescription
      L16_2[4] = L3_2.sIcon
      L16_2[5] = L4_2
      L16_2[6] = L3_2.nMaxStock
      L16_2[7] = L3_2.nFuelCost
      L16_2[8] = L12_2
      L16_2[9] = L5_2
      L16_2[10] = L6_2
      L13_2(L14_2, L15_2, L16_2)
    end
  end
  L7_2 = pairs
  L8_2 = A0_2.CustomData.tEquippedSupport
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  for L10_2, L11_2 in L7_2, L8_2, L9_2 do
    L12_2 = A0_2.CustomData.tSupport[L11_2]
    if L12_2 then
      L13_2 = L12_2.sId
      if L13_2 then
        L16_2 = {}
        L16_2[1] = L10_2
        L16_2[2] = L12_2.sId
        L16_2[3] = L12_2.sName
        L16_2[4] = L12_2.sIcon
        L1_2.CallActionScriptCallback(L1_2, "AddSupportEquipped", L16_2)
      end
    end
  end
  L1_2.SetFlashEventHandler(L1_2, "equipFailed", _ShowUnusableSupportMessage, {})
end

_PopulateSupportDisplay = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = pairs
  L2_2 = MrxSupportData.tSupportData
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = MrxPmc.GetSupportQty(L4_2)
    if L6_2 then
      L7_2 = A0_2.UpdateSupport(A0_2, L5_2)
      if not L7_2 then
        A0_2.AddSupport(A0_2, L5_2)
      end
    end
  end
end

_UpdateSupportData = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L3_2 = "ERROR: No support denial condition specified."
  L4_2 = A0_2.oParentWidget.CustomData.tSupportIdIndex[A1_2]
  L5_2 = nil
  if L4_2 then
    L5_2 = L2_2.CustomData.tSupport[L4_2]
  end
  if L5_2 then
    L6_2 = L5_2.sKey
    if L6_2 then
      L6_2 = MrxSupportData.IsSupportEquippable
      L7_2 = L5_2.sKey
      L6_2, L7_2 = L6_2(L7_2)
      L3_2 = L7_2 or L3_2
      if not L7_2 then
      end
    end
  end
  L9_2 = {}
  L10_2 = "[PDA.Support.EquipFail.Unavailable]"
  L13_2 = "[Generic.Ok]"
  L14_2 = "[Generic.Ok]"
  L9_2[1] = L10_2
  L9_2[2] = L3_2
  L9_2[3] = 0
  L9_2[4] = L13_2
  L9_2[5] = L14_2
  L9_2[6] = nil
  A0_2.CallActionScriptCallback(A0_2, "onlineMessage", L9_2)
end

_ShowUnusableSupportMessage = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = A0_2.oParentWidget
  L3_2 = _ParseString
  L4_2 = A1_2
  L3_2, L4_2 = L3_2(L4_2)
  L8_2 = L2_2
  L7_2 = L2_2.GetOwner
  L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2 = L7_2(L8_2)
  L5_2 = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2)
  if L5_2 then
    L6_2 = type(L3_2)
    if "number" == L6_2 then
      L6_2 = type(L4_2)
      if "string" == L6_2 then
        L6_2 = L2_2.CustomData.tSupportIdIndex[L4_2]
        L7_2 = nil
        if L6_2 then
          L7_2 = L2_2.CustomData.tSupport[L6_2]
        end
        if L7_2 then
          L8_2 = L2_2.CustomData.tEquippedSupport[L3_2]
          if L8_2 then
            L8_2 = L7_2.sName
            L9_2 = L2_2.CustomData.tEquippedSupport[L3_2]
            if L8_2 == L9_2 then
              return
            end
          end
        end
        L8_2 = L2_2.CustomData.tEquippedSupport[L3_2]
        if L8_2 then
          L5_2.RemoveItem(L5_2, L2_2.CustomData.tEquippedSupport[L3_2])
        end
        L8_2 = L2_2.CustomData.tEquippedSupport
        L8_2[L3_2] = nil
        L8_2 = L2_2.CustomData.tEquippedSupportIcons
        L8_2[L3_2] = nil
        if L6_2 and L7_2 then
          L8_2 = {}
          L9_2 = pairs
          L10_2 = L7_2
          L9_2, L10_2, L11_2 = L9_2(L10_2)
          for L12_2, L13_2 in L9_2, L10_2, L11_2 do
            L8_2[L12_2] = L13_2
          end
          L8_2.bAnimate = true
          L8_2.bDontNetSync = true
          L9_2 = L2_2.CustomData.tEquippedSupport
          L9_2[L3_2] = L7_2.sName
          L9_2 = L2_2.CustomData.tEquippedSupportIcons
          L9_2[L3_2] = L7_2.sIcon
          L5_2.AddItem(L5_2, L8_2)
          L9_2 = L2_2.CustomData
          L9_2.bOpenSupportMenuOnExit = true
        end
      end
    end
  end
end

_HandleEquipEvent = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = string.gmatch
  L4_2 = A0_2
  L5_2 = "(%d+)([, ]*)(%w+)"
  L3_2, L4_2, L5_2 = L3_2(L4_2, L5_2)
  for L6_2, L7_2, L8_2 in L3_2, L4_2, L5_2 do
    L1_2 = tonumber(L6_2)
    L2_2 = L8_2
  end
  L3_2 = type(L1_2)
  if "number" == L3_2 then
    L3_2 = type(L2_2)
    if "string" == L3_2 then
      L3_2 = L1_2
      L4_2 = L2_2
      return L3_2, L4_2
    end
  end
  L3_2 = nil
  return L3_2
end

_ParseString = L1_1

function L1_1(A0_2, A1_2)
end

_HandleUnequipEvent = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2
  if not A1_2 then
    L2_2 = nil
    return L2_2
  end
  L2_2 = A0_2.CustomData.tEquippedSupport[A1_2]
  L3_2 = A0_2.CustomData.tEquippedSupportIcons[A1_2]
  return L2_2, L3_2
end

_GetEquippedSupport = L1_1

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L3_2 = type(A2_2)
  if "number" ~= L3_2 then
    return
  end
  if A1_2 then
    L3_2 = A0_2.GetEquippedSupport(A0_2, A2_2)
    if L3_2 == A1_2 then
      return
    end
  end
  L6_2 = A0_2
  L5_2 = A0_2.GetOwner
  L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L5_2(L6_2)
  L3_2 = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
  if L3_2 then
    L4_2 = A0_2.CustomData.tEquippedSupport[A2_2]
    if L4_2 then
      L3_2.RemoveItem(L3_2, A0_2.CustomData.tEquippedSupport[A2_2])
    end
    L4_2 = A0_2.CustomData.tEquippedSupport
    L4_2[A2_2] = nil
    L4_2 = A0_2.CustomData.tEquippedSupportIcons
    L4_2[A2_2] = nil
    if A1_2 then
      L4_2 = A0_2.CustomData.tSupport[A1_2]
      if L4_2 then
        L5_2 = {}
        L6_2 = pairs
        L7_2 = L4_2
        L6_2, L7_2, L8_2 = L6_2(L7_2)
        for L9_2, L10_2 in L6_2, L7_2, L8_2 do
          L5_2[L9_2] = L10_2
        end
        L5_2.bDontNetSync = true
        L6_2 = A0_2.CustomData.tEquippedSupport
        L6_2[A2_2] = L4_2.sName
        L6_2 = A0_2.CustomData.tEquippedSupportIcons
        L6_2[A2_2] = L4_2.sIcon
        L6_2 = A0_2.CustomData
        L6_2.bSupportNeedsEquipping = false
        L3_2.AddItem(L3_2, L5_2)
      end
    end
  end
end

_SetEquippedSupport = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = {}
  L2_2 = A0_2.CustomData.tEquippedSupport[1]
  L3_2 = A0_2.CustomData.tEquippedSupport[2]
  L4_2 = A0_2.CustomData.tEquippedSupport[3]
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  return L1_2
end

ReadEquippedSupport = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  _EquipItemSilent(A0_2, 1, A1_2[1])
  _EquipItemSilent(A0_2, 2, A1_2[2])
  _EquipItemSilent(A0_2, 3, A1_2[3])
end

RestoreEquippedSupport = L1_1

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  if not A2_2 then
    return
  end
  L6_2 = A0_2
  L5_2 = A0_2.GetOwner
  L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L5_2(L6_2)
  L3_2 = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
  if L3_2 then
    L4_2 = type(A1_2)
    if "number" == L4_2 then
      L4_2 = type(A2_2)
      if "string" == L4_2 then
        L4_2 = A0_2.CustomData.tSupport[A2_2]
        if L4_2 then
          L5_2 = A0_2.CustomData.tEquippedSupport[A1_2]
          if L5_2 then
            L5_2 = L4_2.sName
            L6_2 = A0_2.CustomData.tEquippedSupport[A1_2]
            if L5_2 == L6_2 then
              return
            end
          end
        end
        L5_2 = A0_2.CustomData.tEquippedSupport[A1_2]
        if L5_2 then
          L3_2.RemoveItem(L3_2, A0_2.CustomData.tEquippedSupport[A1_2])
        end
        L5_2 = A0_2.CustomData.tEquippedSupport
        L5_2[A1_2] = nil
        L5_2 = A0_2.CustomData.tEquippedSupportIcons
        L5_2[A1_2] = nil
        if A2_2 and L4_2 then
          L5_2 = {}
          L6_2 = pairs
          L7_2 = L4_2
          L6_2, L7_2, L8_2 = L6_2(L7_2)
          for L9_2, L10_2 in L6_2, L7_2, L8_2 do
            L5_2[L9_2] = L10_2
          end
          L5_2.bDontNetSync = true
          L6_2 = A0_2.CustomData.tEquippedSupport
          L6_2[A1_2] = L4_2.sName
          L6_2 = A0_2.CustomData.tEquippedSupportIcons
          L6_2[A1_2] = L4_2.sIcon
          L3_2.AddItem(L3_2, L5_2)
        end
      end
    end
  end
end

_EquipItemSilent = L1_1

function L1_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2
  L4_2 = type(A1_2)
  if "string" == L4_2 then
    L4_2 = type(A2_2)
    if "string" == L4_2 then
      L4_2 = type(A3_2)
      if "number" == L4_2 then
        if A3_2 < 0 then
          L4_2 = A0_2.CustomData.tFactionAttitudes
          L4_2[A1_2] = nil
          L4_2 = true
          return L4_2
        end
        A3_2 = Math.max(Math.min(A3_2, 100), 0)
        L4_2 = A0_2.CustomData.tFactionAttitudes[A1_2]
        if not L4_2 then
          L4_2 = A0_2.CustomData.tFactionAttitudes
          L5_2 = {}
          L5_2[1] = A2_2
          L5_2[2] = A3_2
          L4_2[A1_2] = L5_2
        else
          L4_2 = A0_2.CustomData.tFactionAttitudes[A1_2]
          L4_2[1] = A2_2
          L4_2 = A0_2.CustomData.tFactionAttitudes[A1_2]
          L4_2[2] = A3_2
        end
        L4_2 = true
        return L4_2
      end
    end
  end
  L4_2 = false
  return L4_2
end

SetFactionAttitude = L1_1
nLogSize = 100

function L1_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2
  L5_2 = type(A1_2)
  if "string" ~= L5_2 then
    L5_2 = type(A2_2)
    if "string" ~= L5_2 then
      L5_2 = type(A3_2)
      if "string" ~= L5_2 then
        return
      end
    end
  end
  if "dialog" ~= A1_2 and "objective" ~= A1_2 and "event" ~= A1_2 then
    return
  end
  L5_2 = {}
  L5_2.sType = A1_2
  L5_2.sName = A2_2
  L5_2.sMessage = A3_2
  L6_2 = A4_2 or L6_2
  if not A4_2 then
    L6_2 = "FFFFFF"
  end
  L5_2.sColor = L6_2
  table.insert(A0_2.CustomData.tLogEntries, 1, L5_2)
  while true do
    L6_2 = #A0_2.CustomData.tLogEntries
    L7_2 = nLogSize
    if not (L6_2 > L7_2) then
      break
    end
    table.remove(A0_2.CustomData.tLogEntries)
  end
end

AddLogEntry = L1_1

function L1_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2
  if not A1_2 then
    return
  end
  L4_2 = nil
  L5_2 = A0_2.CustomData.tDataDossiersIndex[A1_2]
  if L5_2 then
    L4_2 = A0_2.CustomData.tDataDossiersIndex[A1_2]
  else
    table.insert(A0_2.CustomData.tDataDossiers, {})
  end
  L4_2.sTitle = A1_2
  L4_2.sText = A2_2
  L4_2.sIcon = A3_2
  L5_2 = A0_2.CustomData.tDataDossiersIndex
  L5_2[A1_2] = L4_2
end

AddDossierEntry = L1_1

function L1_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2
  if not A1_2 then
    return
  end
  L4_2 = nil
  L5_2 = A0_2.CustomData.tDataHelpIndex[A1_2]
  if L5_2 then
    L4_2 = A0_2.CustomData.tDataHelpIndex[A1_2]
  else
    table.insert(A0_2.CustomData.tDataHelp, {})
  end
  L4_2.sTitle = A1_2
  L4_2.sText = A2_2
  L4_2.sIcon = A3_2
  L5_2 = A0_2.CustomData.tDataHelpIndex
  L5_2[A1_2] = L4_2
end

AddHelpEntry = L1_1

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  L3_2 = {}
  L3_2.sCategoryName = A1_2
  L3_2.sIcon = A2_2
  table.insert(A0_2.CustomData.tStatCategories, L3_2)
end

AddStatisticCategory = L1_1

function L1_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2
  L4_2 = A0_2.CustomData.tDataStats[A2_2]
  if L4_2 then
    _UpdateStatisticEntry(A0_2, A2_2, A3_2)
    return
  end
  L4_2 = {}
  L4_2.sCategoryName = A1_2
  L4_2.sText = A2_2
  L4_2.sData = A3_2
  table.insert(A0_2.CustomData.tDataStatsOrdered, L4_2)
  L5_2 = A0_2.CustomData.tDataStats
  L5_2[A2_2] = L4_2
end

AddStatisticEntry = L1_1

function L1_1(A0_2, A1_2, A2_2)
  local L3_2
  L3_2 = A0_2.CustomData.tDataStats[A1_2]
  if L3_2 then
    L3_2 = A0_2.CustomData.tDataStats[A1_2]
    L3_2.sData = A2_2
  end
end

_UpdateStatisticEntry = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L1_2 = A0_2.CustomData.oMapFlash
  L5_2 = {}
  L6_2 = Net.IsConnectedToInternet
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2 = L6_2()
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  L5_2[6] = L11_2
  L5_2[7] = L12_2
  L5_2[8] = L13_2
  L5_2[9] = L14_2
  L5_2[10] = L15_2
  L5_2[11] = L16_2
  L5_2[12] = L17_2
  L5_2[13] = L18_2
  L1_2.CallActionScriptCallback(L1_2, "checkOnline", L5_2)
  L5_2 = {}
  L6_2 = Net.IsServer
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2 = L6_2()
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  L5_2[6] = L11_2
  L5_2[7] = L12_2
  L5_2[8] = L13_2
  L5_2[9] = L14_2
  L5_2[10] = L15_2
  L5_2[11] = L16_2
  L5_2[12] = L17_2
  L5_2[13] = L18_2
  L1_2.CallActionScriptCallback(L1_2, "multiplayerHost", L5_2)
  L5_2 = {}
  L6_2 = Net.IsClient
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2 = L6_2()
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  L5_2[6] = L11_2
  L5_2[7] = L12_2
  L5_2[8] = L13_2
  L5_2[9] = L14_2
  L5_2[10] = L15_2
  L5_2[11] = L16_2
  L5_2[12] = L17_2
  L5_2[13] = L18_2
  L1_2.CallActionScriptCallback(L1_2, "multiplayerClient", L5_2)
  L2_2 = Sys.HaveActiveProfile
  if L2_2 then
    L5_2 = {}
    L6_2 = Sys.HaveActiveProfile
    L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2 = L6_2()
    L5_2[1] = L6_2
    L5_2[2] = L7_2
    L5_2[3] = L8_2
    L5_2[4] = L9_2
    L5_2[5] = L10_2
    L5_2[6] = L11_2
    L5_2[7] = L12_2
    L5_2[8] = L13_2
    L5_2[9] = L14_2
    L5_2[10] = L15_2
    L5_2[11] = L16_2
    L5_2[12] = L17_2
    L5_2[13] = L18_2
    L1_2.CallActionScriptCallback(L1_2, "profileActive", L5_2)
  end
  L2_2 = pairs
  L3_2 = A0_2.CustomData.tFactionAttitudes
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = MrxGuiHudFactionGauge.GetBarValueAndName
    L8_2 = L6_2[2]
    L7_2, L8_2 = L7_2(L8_2)
    L12_2 = {}
    L14_2 = L6_2[1]
    L12_2[1] = L5_2
    L12_2[2] = L14_2
    L12_2[3] = L8_2
    L12_2[4] = L7_2
    L12_2[5] = false
    L1_2.CallActionScriptCallback(L1_2, "AddFactionAttitude", L12_2)
  end
  L5_2 = {}
  L8_2 = "[PDA.Database.Log_All]"
  L5_2[1] = 2
  L5_2[2] = 0
  L5_2[3] = L8_2
  L5_2[4] = "Display all Log Events"
  L5_2[5] = "icon_categories_log"
  L5_2[6] = false
  L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L5_2)
  L5_2 = {}
  L8_2 = "[PDA.Database.Log_Events]"
  L5_2[1] = 2
  L5_2[2] = 0
  L5_2[3] = L8_2
  L5_2[4] = "Filter Message Log by Events"
  L5_2[5] = "icon_categories_events"
  L5_2[6] = false
  L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L5_2)
  L5_2 = {}
  L8_2 = "[PDA.Database.Log_Objectives]"
  L5_2[1] = 2
  L5_2[2] = 0
  L5_2[3] = L8_2
  L5_2[4] = "Filter Message Log by Objectives"
  L5_2[5] = "icon_categories_objectives"
  L5_2[6] = false
  L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L5_2)
  L5_2 = {}
  L8_2 = "[PDA.Database.Log_Dialogue]"
  L5_2[1] = 2
  L5_2[2] = 0
  L5_2[3] = L8_2
  L5_2[4] = "Filter Message Log by Dialogue"
  L5_2[5] = "icon_categories_dialog"
  L5_2[6] = false
  L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L5_2)
  L2_2 = #A0_2.CustomData.tLogEntries
  while 0 < L2_2 do
    L3_2 = A0_2.CustomData.tLogEntries[L2_2]
    L7_2 = {}
    L7_2[1] = L3_2.sType
    L7_2[2] = L3_2.sColor
    L7_2[3] = L3_2.sName
    L7_2[4] = L3_2.sMessage
    L1_2.CallActionScriptCallback(L1_2, "addMessageLog", L7_2)
    L2_2 = L2_2 - 1
  end
  L3_2 = ipairs
  L4_2 = A0_2.CustomData.tDataDossiers
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L11_2 = {}
    L11_2[1] = 3
    L11_2[2] = 0
    L11_2[3] = L7_2.sTitle
    L11_2[4] = L7_2.sText
    L11_2[5] = L7_2.sIcon
    L11_2[6] = false
    L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L11_2)
  end
  L3_2 = {}
  L4_2 = ipairs
  L5_2 = A0_2.CustomData.tStatCategories
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L3_2[L8_2.sCategoryName] = (L7_2 + 2)
    L12_2 = {}
    L12_2[1] = 4
    L12_2[2] = 0
    L12_2[3] = L8_2.sCategoryName
    L12_2[4] = L8_2.sCategoryName
    L12_2[5] = L8_2.sIcon
    L12_2[6] = false
    L1_2.CallActionScriptCallback(L1_2, "AddDatabaseItem", L12_2)
  end
  L4_2 = nil
  L5_2 = ipairs
  L6_2 = A0_2.CustomData.tDataStatsOrdered
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L4_2 = L3_2[L9_2.sCategoryName]
    L13_2 = {}
    L13_2[1] = L4_2
    L13_2[2] = L9_2.sText
    L13_2[3] = L9_2.sData
    L1_2.CallActionScriptCallback(L1_2, "addStats", L13_2)
  end
end

_PopulateDatabaseDisplay = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = A0_2.CustomData
  L1_2.bActive = true
  L1_2 = A0_2.CustomData
  L1_2.oSubtitle = A0_2.GetChildren(A0_2)[1]
  L1_2 = A0_2.CustomData
  L1_2.nCooldownFrames = 0
  L1_2 = MrxGuiBase.ImageWidget
  L1_2 = L1_2.new(L1_2)
  L1_2.SetFullscreen(L1_2, true)
  L6_2 = 0
  L7_2 = 192
  L1_2.SetColor(L1_2, 0, 0, L6_2, L7_2)
  L5_2 = A0_2
  L4_2 = A0_2.GetOwner
  L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L4_2(L5_2)
  L1_2.SetOwner(L1_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
  A0_2.AddChild(A0_2, L1_2)
  L1_2.oParentWidget = A0_2
  L2_2 = MrxGuiBase.FlashWidget
  L2_2 = L2_2.new(L2_2)
  L2_2.SetAnchoring(L2_2, "center", "center")
  L3_2 = 283.33334
  L8_2 = 320 + L3_2
  L9_2 = 480
  L2_2.SetLocation(L2_2, (320 - L3_2), 0, L8_2, L9_2)
  L7_2 = A0_2
  L6_2 = A0_2.GetOwner
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L6_2(L7_2)
  L2_2.SetOwner(L2_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
  A0_2.AddChild(A0_2, L2_2)
  L4_2 = A0_2.CustomData
  L4_2.oMapFlash = L2_2
  L4_2 = L2_2.CustomData
  L4_2.sFile = "topbar"
  L2_2.oParentWidget = A0_2
  A0_2.Open = Open
  A0_2.Close = Close
  A0_2.SetSuppressed = SetSuppressed
  L4_2 = A0_2.CustomData
  L4_2.nSuppressedCount = 0
  L6_2 = "ControllerInput"
  A0_2.SetEventHandler(A0_2, L6_2, _HandleInput)
  L4_2 = A0_2.CustomData
  L4_2.tMapBlips = {}
  L4_2 = A0_2.CustomData
  L4_2.tMissions = {}
  L4_2 = A0_2.CustomData
  L4_2.tMissionIds = {}
  L4_2 = A0_2.CustomData
  L4_2.tRegions = {}
  L4_2 = A0_2.CustomData
  L4_2.bMapMode = false
  L4_2 = A0_2.CustomData
  L4_2.bAllowTrackingChange = true
  L4_2 = A0_2.CustomData
  L4_2.nFramesWithoutInput = -1
  L4_2 = A0_2.CustomData
  L4_2.bHudState = true
  A0_2.AddMapBlip = AddMapBlip
  A0_2.RemoveMapBlip = RemoveMapBlip
  A0_2.AddMapMission = AddMapMission
  A0_2.RemoveMapMission = RemoveMapMission
  A0_2.UpdateMapMission = UpdateMapMission
  A0_2.SetMissionSticky = SetMissionSticky
  A0_2.SetSelectedMission = SetSelectedMission
  A0_2.GetSelectedMission = GetSelectedMission
  A0_2.SetMarker = SetMarker
  A0_2.AddLineRegion = AddLineRegion
  A0_2.RemoveLineRegion = RemoveLineRegion
  A0_2.SetMissionTrackable = SetMissionTrackable
  A0_2.SetMissionTrackCallback = SetMissionTrackCallback
  A0_2.SetMissionChangeAllowed = SetMissionChangeAllowed
  A0_2.SetFakePlayerLocation = SetFakePlayerLocation
  A0_2.SetBeaconTutorialMode = SetBeaconTutorialMode
  L4_2 = A0_2.CustomData
  L4_2.tSupport = {}
  L4_2 = A0_2.CustomData
  L4_2.tSupportOrdered = {}
  L4_2 = A0_2.CustomData
  L4_2.tSupportIdIndex = {}
  L4_2 = A0_2.CustomData
  L4_2.tEquippedSupport = {}
  L4_2 = A0_2.CustomData
  L4_2.tEquippedSupportIcons = {}
  A0_2.AddSupport = AddSupport
  A0_2.RemoveSupport = RemoveSupport
  A0_2.UpdateSupport = UpdateSupport
  A0_2.GetStockpile = GetStockpile
  A0_2.OpenTransitInterface = OpenTransitInterface
  A0_2.GetEquippedSupport = _GetEquippedSupport
  A0_2.SetEquippedSupport = _SetEquippedSupport
  A0_2.ReadEquippedSupport = ReadEquippedSupport
  A0_2.RestoreEquippedSupport = RestoreEquippedSupport
  L4_2 = pairs
  L5_2 = MrxSupportData.tSupportData
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    A0_2.AddSupport(A0_2, L8_2, L7_2)
  end
  L4_2 = A0_2.CustomData
  L4_2.tFactionAttitudes = {}
  L4_2 = A0_2.CustomData
  L4_2.tLogEntries = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataDossiers = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataDossiersIndex = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataHelp = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataHelpIndex = {}
  L4_2 = A0_2.CustomData
  L4_2.tStatCategories = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataStatsOrdered = {}
  L4_2 = A0_2.CustomData
  L4_2.tDataStats = {}
  A0_2.SetFactionAttitude = SetFactionAttitude
  A0_2.AddLogEntry = AddLogEntry
  A0_2.AddDossierEntry = AddDossierEntry
  A0_2.AddHelpEntry = AddHelpEntry
  A0_2.AddStatisticCategory = AddStatisticCategory
  A0_2.AddStatisticEntry = AddStatisticEntry
  A0_2.UpdateStatisticEntry = UpdateStatisticEntry
  A0_2.nAnalogInputHeld = 0
  L4_2 = Event.CreatePersistent
  L5_2 = Event.ScriptEvent
  L6_2 = {}
  L7_2 = "mpPlayerJoin"
  
  function L8_2(A0_3)
    local L1_3, L2_3
    L1_3 = Net.IsServer()
    if L1_3 then
      L1_3 = not Player.IsLocal(A0_3[1])
    end
    return L1_3
  end
  
  L6_2[1] = L7_2
  L6_2[2] = L8_2
  _evPlayerJoin = L4_2(L5_2, L6_2, SendPlayerJoinEvents)
  L4_2 = MrxGuiBase.TextWidget
  L4_2 = L4_2.new(L4_2)
  L4_2.SetAnchoring(L4_2, "left", "top")
  L4_2.SetFont(L4_2, "english_18")
  L4_2.SetJustification(L4_2, "center")
  L4_2.SetLocation(L4_2, 54, 134, 74, 400)
  A0_2.AddChild(A0_2, L4_2)
  L5_2 = A0_2.CustomData
  L5_2.oWindow = L4_2
  MrxGuiBase.AddWidget(L4_2)
  Pg.LoadAsset("pda_titles", "texture")
  MrxGuiBase.AddWidget(L2_2)
  L9_2 = {}
  L9_2[1] = A0_2
  L2_2.SetSwfFile(L2_2, L2_2.CustomData.sFile, _FinishLoadAndClose, L9_2)
end

_Initialize = L1_1

function L1_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Net.IsServer()
  if not L0_2 then
    return
  end
  L2_2 = Player.GetLocalPlayer
  L2_2, L3_2, L4_2, L5_2, L6_2 = L2_2()
  L0_2 = MrxGuiBase.GetWidgetByNameAndOwner("PDA", L2_2, L3_2, L4_2, L5_2, L6_2)
  if L0_2 then
    L1_2 = L0_2.CustomData
    if L1_2 then
      L4_2 = {}
      L5_2 = WifMissionData.GetMissionIndexFromId
      L6_2 = L0_2.CustomData.sSelectedMission
      L5_2, L6_2 = L5_2(L6_2)
      L4_2[1] = L5_2
      L4_2[2] = L6_2
      Net.SendCustomEvent("MrxGuiPda", NETEVENT_SETSELECTEDMISSION, L4_2, true)
    end
  end
end

SendPlayerJoinEvents = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = A0_2.CustomData
  L1_2.bHaveFlash = true
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.Pause(L1_2)
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "TrackBlip", _HandleTrackEvent, {})
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "UntrackBlip", _HandleUntrackEvent, {})
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "cancelContract", _HandleMissionCancel)
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "equip", _HandleEquipEvent, {})
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "unequip", _HandleUnequipEvent, {})
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "closePDA", _HandleCloseEvent, {})
  L1_2 = A0_2.CustomData.oMapFlash
  L1_2.SetFlashEventHandler(L1_2, "currentPage", _HandlePageChangeEvent, {})
end

_FinishLoad = L1_1

function L1_1(A0_2)
  local L1_2, L2_2
  _FinishLoad(A0_2)
  A0_2.Close(A0_2)
end

_FinishLoadAndClose = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2
  L1_2 = 35
  L2_2 = 40
  L3_2 = 1
  L4_2 = Gui.GetMapCorrectionOffset
  if L4_2 then
    L4_2 = Gui.GetMapCorrectionOffset
    L4_2, L5_2, L6_2 = L4_2()
    L3_2 = L6_2
    L2_2 = L5_2
    L1_2 = L4_2
  end
  L4_2 = A0_2.CustomData.oMapFlash
  L5_2 = {}
  L6_2 = nil
  L7_2 = Player.GetAllTargetMarkerPos()
  L8_2 = ipairs
  L9_2 = L7_2
  L8_2, L9_2, L10_2 = L8_2(L9_2)
  for L11_2, L12_2 in L8_2, L9_2, L10_2 do
    L13_2 = L12_2[1]
    if L13_2 then
      L13_2 = nil
      L14_2 = string.format("[PDA.Map.Player:%d]", L11_2)
      if L11_2 == 1 then
        L13_2 = "target1_mc"
      else
        L13_2 = "target2_mc"
      end
      L15_2 = {}
      L18_2 = (L12_2[2] + L1_2) * L3_2
      L19_2 = (L12_2[3] + L2_2) * L3_2
      L15_2[1] = L14_2
      L15_2[2] = L13_2
      L15_2[3] = L18_2
      L15_2[4] = L19_2
      L15_2[5] = 0
      L15_2[6] = L14_2
      L15_2[7] = L14_2
      L15_2[8] = " "
      L15_2[9] = " "
      L15_2[10] = false
      L15_2[11] = " "
      L15_2[12] = " "
      L15_2[13] = true
      L15_2[14] = true
      table.insert(L5_2, L15_2)
    end
  end
  _GuiInternal.AddPdaMapBlips(L4_2.BasicData.uId, L5_2)
end

AddPDATargetMarkers = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2
  L1_2 = 35
  L2_2 = 40
  L3_2 = 1
  L4_2 = Gui.GetMapCorrectionOffset
  if L4_2 then
    L4_2 = Gui.GetMapCorrectionOffset
    L4_2, L5_2, L6_2 = L4_2()
    L3_2 = L6_2
    L2_2 = L5_2
    L1_2 = L4_2
  end
  L4_2 = A0_2.CustomData.oMapFlash
  L5_2 = Player.GetAllTargetMarkerPos()
  L6_2 = ipairs
  L7_2 = L5_2
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    L11_2 = string.format("[PDA.Map.Player:%d]", L9_2)
    L12_2 = L10_2[1]
    if L12_2 then
      L12_2 = nil
      if L9_2 == 1 then
        L12_2 = "target1_mc"
      else
        L12_2 = "target2_mc"
      end
      L15_2 = {}
      L18_2 = (L10_2[2] + L1_2) * L3_2
      L19_2 = (L10_2[3] + L2_2) * L3_2
      L15_2[1] = L11_2
      L15_2[2] = L12_2
      L15_2[3] = L18_2
      L15_2[4] = L19_2
      L15_2[5] = 0
      L15_2[6] = L11_2
      L15_2[7] = L11_2
      L15_2[8] = " "
      L15_2[9] = " "
      L15_2[10] = false
      L15_2[11] = " "
      L15_2[12] = " "
      L15_2[13] = true
      L15_2[14] = false
      L15_2[15] = true
      _GuiInternal.UpdatePdaBlip(L4_2.BasicData.uId, L15_2)
    else
      _GuiInternal.RemovePdaBlip(L4_2.BasicData.uId, L11_2)
    end
  end
end

UpdatePDATargetMarkers = L1_1

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2
  L3_2 = A0_2.CustomData.oMapFlash
  L4_2 = 35
  L5_2 = 40
  L6_2 = 1
  L7_2 = Gui.GetMapCorrectionOffset
  if L7_2 then
    L7_2 = Gui.GetMapCorrectionOffset
    L7_2, L8_2, L9_2 = L7_2()
    L6_2 = L9_2
    L5_2 = L8_2
    L4_2 = L7_2
  end
  L7_2 = nil
  if A2_2 == 1 then
    L7_2 = "player1_mc"
  else
    L7_2 = "player2_mc"
  end
  L8_2 = 0
  L9_2 = Player.GetCamera(A1_2)
  if L9_2 then
    L8_2 = Camera.GetYaw(L9_2)
  end
  L10_2 = string.format("[PDA.Map.Player:%d]", A2_2)
  L11_2 = L7_2
  L12_2 = " "
  L13_2 = A0_2.CustomData.sSelectedMission
  if L13_2 then
    tCurMission = A0_2.CustomData.tMissions[A0_2.CustomData.sSelectedMission]
    L13_2 = tCurMission
    if L13_2 then
      L13_2 = tCurMission.bSuppress
      if not L13_2 then
        L11_2 = tCurMission.sDesc
        L12_2 = "[PDA.Map.CurrentMission]"
      end
    end
  end
  L13_2 = Player.GetCharacter(A1_2)
  L14_2 = nil
  L15_2 = nil
  L16_2 = nil
  L17_2 = A0_2.CustomData.nFakePlayerX
  if L17_2 then
    L14_2 = A0_2.CustomData.nFakePlayerX
    L15_2 = A0_2.CustomData.nFakePlayerY
    L16_2 = A0_2.CustomData.nFakePlayerZ
  else
    L17_2 = Object.GetPosition
    L18_2 = L13_2
    L17_2, L18_2, L19_2 = L17_2(L18_2)
    L16_2 = L19_2
    L15_2 = L18_2
    L14_2 = L17_2
  end
  L19_2 = {}
  L22_2 = (L14_2 + L4_2) * L6_2
  L23_2 = (L16_2 + L5_2) * L6_2
  L19_2[1] = L7_2
  L19_2[2] = L7_2
  L19_2[3] = L22_2
  L19_2[4] = L23_2
  L19_2[5] = -L8_2
  L19_2[6] = L10_2
  L19_2[7] = L11_2
  L19_2[8] = " "
  L19_2[9] = " "
  L19_2[10] = false
  L19_2[11] = " "
  L19_2[12] = L12_2
  L19_2[13] = true
  L19_2[14] = false
  L19_2[15] = false
  _GuiInternal.UpdatePdaBlip(L3_2.BasicData.uId, L19_2)
end

UpdatePlayerMarkers = L1_1
L1_1 = 0

function L2_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = A0_2.CustomData.oMapFlash
  L2_2 = A0_2.GetOwner(A0_2)
  L3_2 = Player.GetAllPlayers()
  L4_2 = ipairs
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    if L8_2 == L2_2 then
      UpdatePlayerMarkers(A0_2, L8_2, L7_2)
      break
    end
  end
  L4_2 = 0
  L5_2 = ipairs
  L6_2 = L3_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    if L9_2 ~= L2_2 then
      UpdatePlayerMarkers(A0_2, L9_2, L8_2)
      L4_2 = L8_2
    end
  end
  L5_2 = L1_1
  if L4_2 < L5_2 then
    _GuiInternal.RemovePdaBlip(L1_2.BasicData.uId, ("player" .. L1_1 .. "_mc"))
  end
  L1_1 = L4_2
end

UpdateAllPlayerMarkers = L2_1
L2_1 = 0

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = (L2_1 + A1_2)
  if 1 < L2_2 then
    L2_1 = 0
    UpdatePDATargetMarkers(A0_2)
    UpdateAllPlayerMarkers(A0_2)
  end
  L2_2 = A0_2.CustomData.nFramesWithoutInput
  if 0 <= L2_2 then
    L2_2 = A0_2.CustomData.bMapMode
    if L2_2 then
      L2_2 = A0_2.CustomData
      L2_2.nFramesWithoutInput = (A0_2.CustomData.nFramesWithoutInput + 1)
      L2_2 = A0_2.CustomData.nFramesWithoutInput
      if 2 < L2_2 then
        L2_2 = A0_2.CustomData
        L2_2.nFramesWithoutInput = -1
        L2_2 = A0_2.CustomData.oMapFlash
        L2_2.HandleLeftAnalogInput(L2_2, 0, 0)
        L2_2.HandleRightAnalogInput(L2_2, 0, 0)
      end
    end
  end
end

_HandlePDAUpdateEvent = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  L2_2 = A0_2.CustomData.bActive
  if L2_2 then
    A0_2.Close(A0_2)
  else
    L4_2 = A0_2
    L3_2 = A0_2.GetOwner
    L3_2, L4_2 = L3_2(L4_2)
    L2_2 = MrxGuiBase.GetCurrentControlHolder(L3_2, L4_2)
    if L2_2 then
      L3_2 = L2_2.GetName(L2_2)
      if "Support Menu" ~= L3_2 then
        goto lbl_21
      end
    end
    A0_2.Open(A0_2)
  end
  ::lbl_21::
end

_HandleToggleEvent = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = A0_2.CustomData.oMapFlash
  L3_2 = A0_2.CustomData.oTransit
  if L3_2 then
    L2_2 = A0_2.CustomData.oTransit
  end
  L3_2 = bExitOnLeft
  if L3_2 then
    L3_2 = MrxGuiBase.Joystick.BUTTON_PAD1_L
    L4_2 = A1_2.ButtonPress
    if L3_2 == L4_2 then
      A0_2.Close(A0_2)
    end
  end
  L3_2 = 0 == A0_2.nAnalogInputHeld
  A0_2.nAnalogInputHeld = 0
  L4_2 = 1.0E-5
  L5_2 = A1_2.LeftAnalogX
  if L5_2 then
    L5_2 = math.abs(A1_2.LeftAnalogX)
    if L4_2 < L5_2 then
      A0_2.nAnalogInputHeld = (A0_2.nAnalogInputHeld + 1)
    end
  end
  L5_2 = A1_2.LeftAnalogY
  if L5_2 then
    L5_2 = math.abs(A1_2.LeftAnalogY)
    if L4_2 < L5_2 then
      A0_2.nAnalogInputHeld = (A0_2.nAnalogInputHeld + 1)
    end
  end
  L5_2 = A1_2.RightAnalogX
  if L5_2 then
    L5_2 = math.abs(A1_2.RightAnalogX)
    if L4_2 < L5_2 then
      A0_2.nAnalogInputHeld = (A0_2.nAnalogInputHeld + 1)
    end
  end
  L5_2 = A1_2.RightAnalogY
  if L5_2 then
    L5_2 = math.abs(A1_2.RightAnalogY)
    if L4_2 < L5_2 then
      A0_2.nAnalogInputHeld = (A0_2.nAnalogInputHeld + 1)
    end
  end
  L5_2 = 0 == A0_2.nAnalogInputHeld
  if L3_2 ~= L5_2 then
    L6_2 = A0_2.CustomData.bMapMode
    if L6_2 then
      L2_2.SetTesselationAllowed(L2_2, L5_2)
      if L5_2 then
        L2_2.HandleLeftAnalogInput(L2_2, 0, 0)
      else
        L9_2 = {}
        L9_2[1] = " "
        L2_2.CallActionScriptCallback(L2_2, "currentPOI", L9_2)
      end
    end
  end
  L6_2 = A0_2.nAnalogInputHeld
  if L6_2 < 0 then
    A0_2.nAnalogInputHeld = 0
  end
  L6_2 = A0_2.CustomData.tControlHistory
  if L6_2 then
    L6_2 = A1_2.ButtonPress
    if L6_2 then
      L6_2 = #A0_2.CustomData.tControlHistory
      L7_2 = _kMaxCodeLength
      if L6_2 < L7_2 then
        table.insert(A0_2.CustomData.tControlHistory, A1_2.ButtonPress)
      end
    end
  end
  L6_2 = A0_2.CustomData.tControlHistory
  if L6_2 then
    L6_2 = A0_2.CustomData.oWindow
    if L6_2 then
      L6_2 = Pg.WereCheatsUsed
      if L6_2 then
        L6_2 = Pg.WereCheatsUsed()
        if L6_2 then
          L6_2 = A0_2.CustomData.oTransit
          if not L6_2 then
            L6_2 = A0_2.CustomData.oWindow
            L8_2 = _GenerateInputString
            L9_2 = A0_2.CustomData.tControlHistory
            L8_2, L9_2, L10_2 = L8_2(L9_2)
            L6_2.SetText(L6_2, L8_2, L9_2, L10_2)
          end
        end
      end
    end
  end
  L2_2.EventHandlers.ControllerInput(L2_2, A1_2)
  L6_2 = A0_2.CustomData.bMapMode
  if not L6_2 then
    L6_2 = A0_2.CustomData.oTransit
    if not L6_2 then
      goto lbl_211
    end
  end
  L6_2 = A0_2.CustomData
  L6_2.nFramesWithoutInput = 0
  L6_2 = A1_2.LeftAnalogX
  if not L6_2 then
    L6_2 = A1_2.LeftAnalogY
    if not L6_2 then
      goto lbl_186
    end
  end
  L7_2 = L2_2
  L6_2 = L2_2.HandleLeftAnalogInput
  L8_2 = A1_2.LeftAnalogX
  if not L8_2 then
    L8_2 = 0
  end
  L9_2 = A1_2.LeftAnalogY
  if not L9_2 then
    L9_2 = 0
  end
  L6_2(L7_2, L8_2, L9_2)
  goto lbl_190
  ::lbl_186::
  L2_2.HandleLeftAnalogInput(L2_2, 0, 0)
  ::lbl_190::
  L6_2 = A1_2.RightAnalogX
  if not L6_2 then
    L6_2 = A1_2.RightAnalogY
    if not L6_2 then
      goto lbl_207
    end
  end
  L7_2 = L2_2
  L6_2 = L2_2.HandleRightAnalogInput
  L8_2 = A1_2.RightAnalogX
  if not L8_2 then
    L8_2 = 0
  end
  L9_2 = A1_2.RightAnalogY
  if not L9_2 then
    L9_2 = 0
  end
  L6_2(L7_2, L8_2, L9_2)
  goto lbl_211
  ::lbl_207::
  L2_2.HandleRightAnalogInput(L2_2, 0, 0)
  ::lbl_211::
end

_HandleInput = L3_1
_tInputStrings = nil

function L3_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = #A0_2
  L2_2 = _kMaxCodeLength - 1
  if L1_2 > L2_2 then
    L1_2 = " "
    return L1_2
  end
  sReturn = ""
  L1_2 = pairs
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = _tInputStrings[L5_2]
    if L6_2 then
      sReturn = (sReturn .. _tInputStrings[L5_2] .. "[n]")
    else
      L6_2 = " "
      return L6_2
    end
  end
  L1_2 = sReturn
  return L1_2
end

_GenerateInputString = L3_1

function L3_1(A0_2)
  local L1_2
  if not A0_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_L
  if A0_2 >= L1_2 then
    L1_2 = MrxGuiBase.Joystick.BUTTON_R_STICK_D
    if A0_2 <= L1_2 then
      L1_2 = true
      return L1_2
    end
  end
  L1_2 = false
  return L1_2
end

IsAnalog = L3_1

function L3_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = A0_2.oParentWidget
  L1_2.Close(L1_2)
end

_HandleCloseEvent = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = A0_2.oParentWidget
  if "Map" == A1_2 then
    L3_2 = L2_2.CustomData
    L3_2.bMapMode = true
  else
    L3_2 = L2_2.CustomData.bMapMode
    if L3_2 then
      L3_2 = L2_2.CustomData
      L3_2.nFramesWithoutInput = -1
      L3_2 = L2_2.CustomData.oMapFlash
      L3_2.HandleLeftAnalogInput(L3_2, 0, 0)
      L3_2.HandleLeftAnalogInput(L3_2, 0, 0)
    end
    L3_2 = L2_2.CustomData
    L3_2.bMapMode = false
  end
end

_HandlePageChangeEvent = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L2_2 = {}
  L3_2 = 1
  L4_2 = string.gmatch
  L5_2 = A1_2
  L6_2 = "-*%d+"
  L4_2, L5_2, L6_2 = L4_2(L5_2, L6_2)
  for L7_2 in L4_2, L5_2, L6_2 do
    L2_2[L3_2] = tonumber(L7_2)
    L3_2 = L3_2 + 1
  end
  L4_2 = L2_2[1]
  if L4_2 then
    L4_2 = L2_2[2]
    if L4_2 then
      L4_2 = L2_2[1] * 2
      L5_2 = 0
      L6_2 = L2_2[2] * 2
      L7_2 = Pg.IsPointInBoundary
      if L7_2 then
        L7_2 = pairs
        L8_2 = WifVzRegionNames.tBoundaryList
        L7_2, L8_2, L9_2 = L7_2(L8_2)
        for L10_2, L11_2 in L7_2, L8_2, L9_2 do
          L12_2 = Pg.GetGuidByName(L10_2)
          if L12_2 then
            L13_2 = Pg.IsPointInBoundary(L4_2, L5_2, L6_2, L12_2)
            if L13_2 then
              L16_2 = {}
              L16_2[1] = L11_2
              A0_2.CallActionScriptCallback(A0_2, "currentPOI", L16_2)
              return
            end
          end
        end
      end
    end
  end
  L7_2 = {}
  L7_2[1] = "Venezuela"
  A0_2.CallActionScriptCallback(A0_2, "currentPOI", L7_2)
end

_HandleMapLocationEvent = L3_1
_tFactionNameLookup = false

function L3_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  bExitOnLeft = true
  L0_2 = Gui.IsPdaOnSelect
  if L0_2 then
    L0_2 = Gui.IsPdaOnSelect()
    if L0_2 then
      bExitOnLeft = false
    end
  end
  L0_2 = {}
  L0_2.AN = "[0x8c648d92]"
  L0_2.PR = "[0x151ea816]"
  L0_2.OC = "[0x0375c825]"
  L0_2.GR = "[0xec76433f]"
  L0_2.CH = "[0x0b54aa0b]"
  L0_2.VZ = "[0xa7953946]"
  L0_2.PMC = "[0xeb4191d9]"
  _tFactionNameLookup = L0_2
  L0_2 = Event.CreatePersistent
  L1_2 = Event.ScriptEvent
  L2_2 = {}
  L3_2 = "mpPlayerJoin"
  
  function L4_2(A0_3)
    local L1_3, L2_3
    L1_3 = Net.IsServer()
    if L1_3 then
      L1_3 = not Player.IsLocal(A0_3[1])
    end
    return L1_3
  end
  
  L2_2[1] = L3_2
  L2_2[2] = L4_2
  L0_2(L1_2, L2_2, OnPlayerJoin, {})
  _tInputStrings = {}
  L0_2 = _tInputStrings
  L0_2[MrxGuiBase.Joystick.BUTTON_PAD1_U] = "[dpad up]"
  L0_2 = _tInputStrings
  L0_2[MrxGuiBase.Joystick.BUTTON_PAD1_D] = "[dpad down]"
  L0_2 = _tInputStrings
  L0_2[MrxGuiBase.Joystick.BUTTON_PAD1_L] = "[dpad left]"
  L0_2 = _tInputStrings
  L0_2[MrxGuiBase.Joystick.BUTTON_PAD1_R] = "[dpad right]"
  L0_2 = Gui.SetMapCorrectionOffset
  if L0_2 then
    Gui.SetMapCorrectionOffset(0, 0, 1.1377778)
  end
end

Init = L3_1
