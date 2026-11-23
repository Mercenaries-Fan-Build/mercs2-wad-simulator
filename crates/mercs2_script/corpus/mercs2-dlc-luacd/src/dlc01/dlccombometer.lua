local L0_1, L1_1, L2_1
import("MrxUtil", false)

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  nCurDestroyedBldgs = 0
  nTotalBldgCount = 0
  nPeakCombo = 1
  nPrevPeakCombo = 1
  L3_2 = {}
  L3_2.uPlayer = A1_2
  L3_2.uTimeStamp = Sys.MainTimeStamp()
  L3_2.nCombo = 1
  L3_2.nScore = 0
  L3_2.fMeter = 0
  L3_2.fComboDecayTime = MrxUtil.SetDefault(A2_2, 5)
  L3_2.uFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(L3_2.uFilter, "Vehicle||Building")
  L6_2 = {}
  L6_2[1] = L3_2.uFilter
  L8_2 = {}
  L8_2[1] = A0_2
  L8_2[2] = L3_2
  L3_2.eObjectDestroyed = Event.CreatePersistent(Event.ObjectDeath, L6_2, Combo_ObjectDestroyed, L8_2)
  Combo_Display(L3_2)
  return L3_2
end

Combo_Init = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2
  L3_2 = Object.GetPosition
  L4_2 = A2_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  L6_2 = math.randf(-2, 2)
  L7_2 = math.randf(-2, 2)
  L8_2 = ""
  L9_2 = 0
  L10_2 = Object.GetMaxHealth(A2_2)
  L11_2 = Object.HasLabel(A2_2, "Vehicle")
  if not L11_2 then
    L11_2 = Object.HasLabel(A2_2, "Building")
    if not L11_2 then
      goto lbl_81
    end
  end
  A1_2.nCombo = (A1_2.nCombo + 1)
  L11_2 = A1_2.nCombo
  L12_2 = nPeakCombo
  if L11_2 > L12_2 then
    nPrevPeakCombo = nPeakCombo
    nPeakCombo = A1_2.nCombo
    L11_2 = MessageBox
    L11_2.AddMessage(L11_2, ("[green][DLCCon004.UI.NewPeakCombo] X" .. nPeakCombo), 0, 3, 0, nil, true, nil)
  end
  nCurDestroyedBldgs = (nCurDestroyedBldgs + 1)
  L11_2 = Object.GetParent(A2_2)
  L12_2 = Pg.GetGuidByName("_DLC_vzoutpost_bld_guardtower")
  if L11_2 == L12_2 then
    return
  else
    L12_2 = Object.HasLabel(A2_2, "uberVehicle")
    if not L12_2 then
      L9_2 = L10_2 / 10
    end
  end
  ::lbl_81::
  if 50 <= L9_2 then
    L11_2 = math.floor((L9_2 / 50))
    L12_2 = 1
    L13_2 = L11_2
    L14_2 = 1
    for L15_2 = L12_2, L13_2, L14_2 do
      Pg.Spawn("DLCCon004_Cash_XXL_05", (L3_2 + math.randf(-2, 2)), (L4_2 + 4), (L5_2 + math.randf(-2, 2)))
    end
  end
  L11_2 = L9_2 % 50
  if 10 <= L11_2 then
    L12_2 = math.floor((L11_2 / 10))
    L13_2 = 1
    L14_2 = L12_2
    L15_2 = 1
    for L16_2 = L13_2, L14_2, L15_2 do
      Pg.Spawn("DLCCon004_Cash_L_03", (L3_2 + math.randf(-2, 2)), (L4_2 + 4), (L5_2 + math.randf(-2, 2)))
    end
  end
  L12_2 = L11_2 % 10
  if 0 < L12_2 then
    L13_2 = 1
    L14_2 = L12_2
    L15_2 = 1
    for L16_2 = L13_2, L14_2, L15_2 do
      Pg.Spawn("DLCCon004_Cash_S_01", (L3_2 + math.randf(-2, 2)), (L4_2 + 4), (L5_2 + math.randf(-2, 2)))
    end
  end
  Pg.Spawn("Explosion (TEST)", L3_2, L4_2, L5_2)
  Combo_StartDecay(A1_2)
end

Combo_ObjectDestroyed = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  Sys.TimeStampMark(A0_2.uTimeStamp)
  L1_2 = A0_2.eTimer
  if not L1_2 then
    L3_2 = {}
    L3_2[1] = 0.1
    L5_2 = {}
    L5_2[1] = A0_2
    A0_2.eTimer = Event.CreatePersistent(Event.TimerRelative, L3_2, Combo_Update, L5_2)
  end
end

Combo_StartDecay = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2.fMeter = ((A0_2.fComboDecayTime - Sys.TimeStampGetElapsed(A0_2.uTimeStamp)) / A0_2.fComboDecayTime)
  A0_2.fMeter = Math.floor((100 * A0_2.fMeter))
  L1_2 = A0_2.fMeter
  if L1_2 <= 0 then
    L1_2 = A0_2.nCombo
    if L1_2 == 1 then
      A0_2.fMeter = 0
      Event.Delete(A0_2.eTimer)
      A0_2.eTimer = nil
    else
      A0_2.fMeter = 100
      Sys.TimeStampMark(A0_2.uTimeStamp)
      A0_2.nCombo = (A0_2.nCombo - 1)
    end
  end
  Combo_Display(A0_2)
end

Combo_Update = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = A0_2.nCombo
  if 5 < L1_2 then
    L1_2 = Hud.ObjectiveTray
    L3_2 = {}
    L3_2.nSlot = 1
    L3_2.sText = ("x" .. A0_2.nCombo .. "[green][bar" .. A0_2.fMeter .. "]")
    L1_2.SetSlotToText(L1_2, L3_2)
  else
    L1_2 = A0_2.nCombo
    if 2 < L1_2 then
      L1_2 = A0_2.nCombo
      if L1_2 <= 5 then
        L1_2 = Hud.ObjectiveTray
        L3_2 = {}
        L3_2.nSlot = 1
        L3_2.sText = ("x" .. A0_2.nCombo .. "[yellow][bar" .. A0_2.fMeter .. "]")
        L1_2.SetSlotToText(L1_2, L3_2)
    end
    else
      L1_2 = A0_2.nCombo
      if L1_2 <= 2 then
        L1_2 = Hud.ObjectiveTray
        L3_2 = {}
        L3_2.nSlot = 1
        L3_2.sText = ("x" .. A0_2.nCombo .. "[red][bar" .. A0_2.fMeter .. "]")
        L1_2.SetSlotToText(L1_2, L3_2)
      end
    end
  end
end

Combo_Display = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = Event.CreatePersistent
  L2_2 = Event.ScriptEvent
  L3_2 = {}
  L4_2 = "SurvivalMode"
  
  function L5_2(A0_3)
    local L1_3, L2_3, L3_3
    L1_3 = A0_3[1] == Player.GetCharacter(A0_2.uPlayer)
    return L1_3
  end
  
  L3_2[1] = L4_2
  L3_2[2] = L5_2
  L5_2 = {}
  L6_2 = A0_2
  L5_2[1] = L6_2
  A0_2.ePlayerDamage = L1_2(L2_2, L3_2, Combo_PlayerDamage, L5_2)
  L3_2 = {}
  L4_2 = Player.GetCharacter
  L5_2 = A0_2.uPlayer
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  L3_2[1] = L4_2
  L3_2[2] = L5_2
  L3_2[3] = L6_2
  L5_2 = {}
  L5_2[1] = A0_2
  A0_2.ePlayerDeath = Event.Create(Event.ObjectDeath, L3_2, Combo_PlayerDamage, L5_2)
end

Combo_PlayerDamageSetup = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = MessageBox
  L2_2.AddMessage(L2_2, "[red]Combo lost!", 0, 4, 0, nil, true, nil)
  Combo_Reset(A0_2)
end

Combo_PlayerDamage = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.fMeter = 0
  Event.Delete(A0_2.eTimer)
  A0_2.eTimer = nil
  Event.Delete(A0_2.ePlayerDamage)
  A0_2.ePlayerDamage = nil
  Event.Delete(A0_2.ePlayerDeath)
  A0_2.ePlayerDeath = nil
  A0_2.nScore = 0
  A0_2.nCombo = 1
end

Combo_Reset = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  Event.Delete(A0_2.eTimer)
  A0_2.eTimer = nil
  Event.Delete(A0_2.eObjectDestroyed)
  Event.Delete(A0_2.ePlayerDamage)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
end

Combo_Cleanup = L0_1
