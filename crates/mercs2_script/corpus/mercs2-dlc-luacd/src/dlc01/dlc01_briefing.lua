local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1
import("MrxCinematic", false)
import("WifEquipmentData", false)
import("MrxFactionManager", false)
import("MrxGui", false)
import("MrxGuiDialogBox", false)
import("MrxUtil", false)
import("MrxVoSequence", false)
import("WifBriefingData", false)
import("WifHints", false)
import("WifMissionData", false)
import("WifMissionFlow", false)
import("WifPmcInterior", false)
import("WifRecommendationData", false)
import("MrxPlayer", false)
import("MrxPlayState", false)
import("MrxState", false)
import("MrxTransit", false)
import("MrxSupportTransit", false)
import("MrxStarterManager", false)
import("WifHqData", false)
import("MrxRewardData", false)
import("MrxPmc", false)
import("MrxSoundBanks", false)
import("WifStarterData", false)
import("MrxShop", false)
import("DLC01_MissionHub", true)
CHEAP_GREETING = 1
CHEAP_SPECIALCASEGREETING = 2
CHEAP_STARTINTRO = 3
CHEAP_JOBREQUEST = 4
CHEAP_JOBACCEPT = 5
CHEAP_JOBDECLINE = 6
CHEAP_WAGERBEGINWIN = 7
CHEAP_WAGERBEGINLOSE = 8
CHEAP_WAGERWON = 9
CHEAP_WAGERLOST = 10
CHEAP_WAGERCHICKENSUIT = 11
CHEAP_HINT = 12
CHEAP_GOODBYE = 13
CHEAP_CONFIRM = 14
CHEAP_DECLINE = 15
CHEAP_INTRO = 16
CHEAP_PMCWAGER = 17
L0_1 = {}
L1_1 = {}
L2_1 = {}
L2_1[1] = "Chris.Greeting01"
L2_1[2] = "Chris.Greeting02"
L2_1[3] = "Chris.Greeting03"
L1_1.Chris = L2_1
L2_1 = {}
L2_1[1] = "Mattias.Greeting01"
L2_1[2] = "Mattias.Greeting02"
L2_1[3] = "Mattias.Greeting03"
L1_1.Mattias = L2_1
L2_1 = {}
L2_1[1] = "Jen.Greeting01"
L2_1[2] = "Jen.Greeting02"
L2_1[3] = "Jen.Greeting03"
L1_1.Jennifer = L2_1
L0_1.tJobRequest = L1_1
L1_1 = {}
L2_1 = {}
L2_1[1] = "Chris.Yes01"
L2_1[2] = "Chris.Yes02"
L2_1[3] = "Chris.Yes03"
L1_1.Chris = L2_1
L2_1 = {}
L2_1[1] = "Mattias.Yes01"
L2_1[2] = "Mattias.Yes02"
L2_1[3] = "Mattias.Yes03"
L1_1.Mattias = L2_1
L2_1 = {}
L2_1[1] = "Jen.Yes01"
L2_1[2] = "Jen.Yes02"
L2_1[3] = "Jen.Yes03"
L1_1.Jennifer = L2_1
L0_1.tJobAccept = L1_1
L1_1 = {}
L2_1 = {}
L2_1[1] = "Chris.No01"
L2_1[2] = "Chris.No02"
L2_1[3] = "Chris.No03"
L1_1.Chris = L2_1
L2_1 = {}
L2_1[1] = "Mattias.No01"
L2_1[2] = "Mattias.No02"
L2_1[3] = "Mattias.No03"
L1_1.Mattias = L2_1
L2_1 = {}
L2_1[1] = "Jen.No01"
L2_1[2] = "Jen.No02"
L2_1[3] = "Jen.No03"
L1_1.Jennifer = L2_1
L0_1.tJobDecline = L1_1
_tHeroWrapperVo = L0_1
_tViewedIntros = {}
L0_1 = {}
L1_1 = {}
L1_1.nAngle = 0
L1_1.nStartNear = 0
L1_1.nEndNear = 0.3
L1_1.nStartFar = 4
L1_1.nEndFar = 10
L1_1.nBlur = 0.5
L0_1.DOF = L1_1
L1_1 = {}
L1_1.nAngle = 55
L0_1.FOV = L1_1
_tDefaultCameraEffects = L0_1
_ClientMenuBox = nil
_ClientJoinEvent = nil
_ClientMenuPending = nil

function L0_1()
  local L0_2, L1_2
  _CheckAssets()
end

Deinit = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  _oStarter = A0_2
  L1_2 = _ClientJoinEvent
  if not L1_2 then
    L1_2 = Event.CreatePersistent
    L2_2 = Event.ScriptEvent
    L3_2 = {}
    L4_2 = "mpPlayerJoin"
    
    function L5_2(A0_3)
      local L1_3, L2_3
      L1_3 = Net.IsServer()
      if L1_3 then
        L1_3 = not Player.IsLocal(A0_3[1])
      end
      return L1_3
    end
    
    L3_2[1] = L4_2
    L3_2[2] = L5_2
    _ClientJoinEvent = L1_2(L2_2, L3_2, SendPlayerJoinEvents)
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    L1_2 = _oStarter
    L1_2 = L1_2.GetName(L1_2)
    L2_2 = MrxShop.GetIndexedShopList(A0_2)
    L3_2 = 1
    L4_2 = {}
    L4_2[L3_2] = GetActorGuid("HqInterior")
    L3_2 = L3_2 + 1
    L4_2[L3_2] = GetActorGuid("Starter")
    L3_2 = L3_2 + 1
    if L1_2 then
      L5_2 = WifStarterData[L1_2].tActors
      if L5_2 then
        L6_2 = pairs
        L7_2 = L5_2
        L6_2, L7_2, L8_2 = L6_2(L7_2)
        for L9_2, L10_2 in L6_2, L7_2, L8_2 do
          if L9_2 ~= "Starter" then
            L4_2[L3_2] = GetActorGuid(L9_2)
            L3_2 = L3_2 + 1
          end
        end
      end
    end
    Net.SetBriefingStarters(MrxStarterManager.GetStarterIndexFromName(L1_2), L4_2, L2_2)
  end
end

SetStarter = L0_1

function L0_1(A0_2)
  local L1_2
  _tBriefingWrapper = A0_2
end

SetBriefingWrapper = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = Net.IsServer()
  if not L0_2 then
    return
  end
  L0_2 = _ClientMenuBox
  if L0_2 then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_DISPLAYMENU, {})
  end
end

SendPlayerJoinEvents = L0_1

function L0_1()
  local L0_2, L1_2
  _bLoadingBriefingAssets = L0_2
  _bBriefingAssetsLoaded = true
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
  L0_2 = _tForceUnloadBriefingAssets
  if L0_2 then
    NetSafeUnloadBriefingAssets(_tForceUnloadBriefingAssets)
    _tForceUnloadBriefingAssets = nil
  end
end

NetSafeBriefingAssetsLoaded = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _bBriefingAssetsLoaded
  return L0_2
end

NetSafeAreBriefingAssetsLoaded = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = _bLoadingBriefingAssets
  if not L1_2 then
    _bBriefingAssetsLoaded = nil
    L4_2 = {}
    L4_2[1] = A0_2
    L4_2[2] = NetSafeBriefingAssetsLoaded
    MrxState.Enter(MrxState.STATE_WAITFORGAME, LoadTableOfAssets, L4_2)
  end
  _bLoadingBriefingAssets = true
end

NetSafeLoadBriefingAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  L1_2 = _bLoadingBriefingAssets
  if L1_2 then
    _tForceUnloadBriefingAssets = A0_2
  else
    _bLoadingBriefingAssets = nil
    _bBriefingAssetsLoaded = nil
    UnloadTableOfAssets(A0_2)
  end
end

NetSafeUnloadBriefingAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = pairs
  L3_2 = _tFlashObjects
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = String.GetHash(L5_2)
    if L7_2 == A0_2 then
      L1_2 = L5_2
    end
  end
  if L1_2 then
    _ShowFlashObject(L1_2)
  end
end

NetSafeShowFlashBriefing = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = pairs
  L3_2 = _tFlashObjects
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = String.GetHash(L5_2)
    if L7_2 == A0_2 then
      L1_2 = L5_2
    end
  end
  if L1_2 then
    _RemoveFlashObject(L1_2)
  end
end

NetSafeRemoveFlashBriefing = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _starterLoaded
  return L0_2
end

NetSafeIsStarterLoaded = L0_1

function L0_1()
  local L0_2, L1_2
  MrxState.SetQuickFade(true)
  _starterLoaded = true
end

NetSafeStarterLoaded = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L3_2 = MrxPlayer.GetRemoteOutfit()
  if not L3_2 then
    L3_2 = 0
  end
  L4_2 = WifPmcInterior.IsCostumeBriefingSafe((L3_2 + 1))
  if not L4_2 then
    L4_2 = Player.GetPrimaryCharacter()
    L5_2 = MrxUtil.GetCharacterIdentity(L4_2)
    L6_2 = {}
    L6_2.mattias = "pmc_hum_mattias"
    L6_2.jennifer = "pmc_hum_jen"
    L6_2.chris = "pmc_hum_chris"
    Player.SetOutfit(L4_2, L6_2[L5_2])
    L9_2 = {}
    L9_2[1] = L4_2
    L9_2[2] = "awake"
    L9_2[3] = 0
    L11_2 = {}
    L11_2[1] = A0_2
    L11_2[2] = A1_2
    L11_2[3] = A2_2
    Event.Create(Event.ObjectIsReady, L9_2, NetSafeSetStarter2, L11_2)
  else
    NetSafeSetStarter2(A0_2, A1_2, A2_2)
  end
end

NetSafeSetStarter = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  MrxState.SetQuickFade(false)
  _starterLoaded = nil
  Vehicle.Exit(A1_2[1], A1_2[2])
  MrxShop.SetIndexedShopList(A2_2)
  _oStarter = MrxStarterManager.RequestStarter(MrxStarterManager.GetStarterNameFromIndex(A0_2))
  L4_2 = _oStarter
  L4_2.SetActor(L4_2, A1_2[2])
  L4_2 = _oStarter
  L4_2.Load(L4_2, NetSafeStarterLoaded)
  L4_2 = 3
  L5_2 = Object.GetName(A1_2[1])
  if not L5_2 then
    Object.SetName(A1_2[1], "HqInterior")
  end
  if L3_2 then
    L5_2 = WifStarterData[L3_2].tActors
    if L5_2 then
      L6_2 = pairs
      L7_2 = L5_2
      L6_2, L7_2, L8_2 = L6_2(L7_2)
      for L9_2, L10_2 in L6_2, L7_2, L8_2 do
        if L9_2 ~= "Starter" then
          L11_2 = A1_2[L4_2]
          if L11_2 then
            Object.SetName(A1_2[L4_2], L9_2)
          end
          L4_2 = L4_2 + 1
        end
      end
    end
  end
  L6_2 = _oStarter
  L7_2 = L6_2
  L6_2 = L6_2.GetBriefingWrapper
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2 = L6_2(L7_2)
  SetBriefingWrapper(L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2)
  _nBaseShadowDistance = Graphics.GetShadowBaseDistance()
  Graphics.SetShadowBaseDistance(2)
  L5_2 = _oStarter
  L5_2 = L5_2.IsPmcStarter(L5_2)
  if L5_2 then
    L6_2 = {}
    L6_2[1] = "Player1"
    L6_2[2] = "Starter"
    _SaveActorsOriginalPositions(L6_2)
    L5_2 = _oStarter
    L6_2 = WifPmcInterior.GetStarterBriefingLocs(L5_2.GetName(L5_2))
    L8_2 = {}
    L8_2.Player1 = L6_2[2]
    L8_2.Starter = L6_2[1]
    _AttachActorsToLocations(L8_2)
    L8_2 = Player.GetSecondaryCharacter()
    L9_2 = Pg.GetGuidByName
    L10_2 = L6_2[2]
    L9_2, L10_2, L11_2, L12_2, L13_2 = L9_2(L10_2)
    Object.SetTransformToObject(L8_2, L9_2, L10_2, L11_2, L12_2, L13_2)
  else
    L6_2 = {}
    L6_2.Player1 = "hp_playerA"
    L6_2.Starter = "hp_starter"
    _AttachActorsToHardpoints(L6_2)
    Object.SetTransformToObject(Player.GetSecondaryCharacter(), GetActorGuid("HqInterior"), "hp_playerA")
  end
  _SetupPlayers(false)
  Gui.EnablePlayerMarkers(false)
  L5_2 = _oStarter
  L5_2 = L5_2.IsBoss(L5_2)
  if not L5_2 then
    L6_2 = GetActorGuid
    L7_2 = "Starter"
    L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2 = L6_2(L7_2)
    Human.DisableWeapons(L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2)
  end
  VO.Cancel()
  VO.SetCinematicMode(true)
  _BindFaceAnim(true, "Player1")
  _BindFaceAnim(true, "Starter")
  L6_2 = {}
  L6_2[1] = "Player1"
  L6_2[2] = "Starter"
  _SetActorsToDefaultPose(L6_2, false)
  L5_2 = _oStarter
  L5_2 = L5_2.IsBoss(L5_2)
  if not L5_2 then
    L5_2 = _oStarter
    L5_2 = L5_2.IsPmcStarter(L5_2)
    if L5_2 then
      L6_2 = {}
      L7_2 = {}
      L7_2.sName = "OverTheShoulderLeft"
      L7_2.sBaseActor = "Starter"
      L7_2.sTargetActor = "Player1"
      L6_2.tShot = L7_2
      _ProcessCameraSettings(L6_2)
    end
  end
  L5_2 = _oStarter
  L5_2 = L5_2.GetHq(L5_2)
  if L5_2 then
    L6_2 = WifHqData.GetHqConfigFromId(L5_2)
    if L6_2 then
      Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), L6_2.sAtmosphere)
    end
  end
  L6_2 = _ClientMenuPending
  if L6_2 then
    _DisplayClientMenu()
  end
end

NetSafeSetStarter2 = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  MrxState.SetQuickFade(false)
  _starterLoaded = nil
  L0_2 = _oStarter
  if L0_2 then
    L0_2 = _oStarter
    L0_2 = L0_2.GetActor(L0_2)
    L1_2 = _oStarter
    L1_2.Unload(L1_2)
    L1_2 = _oStarter
    L1_2.SetActor(L1_2, L0_2)
  end
  MrxShop.SetIndexedShopList(nil)
  _ClearAllFlashObjects()
  _End()
end

NetSafeClearStarter = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  MrxState.SetQuickFade(true)
  _Fade(false)
  L2_2 = _ClientWaitBox
  if L2_2 then
    L2_2 = _ClientWaitBox
    L2_2.Close(L2_2)
    _ClientWaitBox = nil
  end
  _bNetSafeSpielLoaded = nil
  _sSelectedMission = WifMissionData.GetMissionIdFromIndex(A0_2)
  L3_2 = _tBriefings
  if not L3_2 then
    _tBriefings = {}
  end
  L3_2 = _tBriefings
  L3_2[_sSelectedMission] = {}
  L3_2 = _tBriefings[_sSelectedMission]
  L4_2 = WifBriefingData[L2_2]
  if not L4_2 then
    L4_2 = {}
  end
  L3_2.tConfig = L4_2
  L3_2 = 1
  L4_2 = _tBriefings[_sSelectedMission].tConfig.tActors
  if L4_2 then
    L5_2 = pairs
    L6_2 = L4_2
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    for L8_2, L9_2 in L5_2, L6_2, L7_2 do
      if L8_2 ~= "Starter" then
        L10_2 = Object.GetName(A1_2[L3_2])
        if not L10_2 then
          Object.SetName(A1_2[L3_2], L8_2)
        end
        L3_2 = L3_2 + 1
      end
    end
  end
  L5_2 = GetSpielFileName(L2_2)
  L6_2 = WifBriefingData[L2_2]
  if not L6_2 then
    L6_2 = {}
  end
  if L6_2 then
    L7_2 = L6_2.tPositions
    if L7_2 then
      _AttachActorsToHardpoints(L6_2.tPositions)
    end
  end
  dynamic_import(L5_2, _FileLoaded)
end

NetSafeLoadSpiel = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _bNetSafeSpielLoaded
  return L0_2
end

NetSafeIsSpielLoaded = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  _bNetSafeSpielLoaded = L1_2
  _ClearAllFlashObjects()
  _CinematicComplete()
  MrxState.SetQuickFade(false)
end

NetSafeUnloadSpiel = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  _StopCheapCinematic()
  L2_2 = CHEAP_CONFIRM
  if A0_2 ~= L2_2 then
    L2_2 = CHEAP_DECLINE
    if A0_2 ~= L2_2 then
      goto lbl_51
    end
  end
  L2_2 = _GetSelectedBriefingConfig()
  if L2_2 then
    L3_2 = _bNetSafeSpielLoaded
    if L3_2 then
      L3_2 = nil
      L4_2 = CHEAP_CONFIRM
      if A0_2 == L4_2 then
        L3_2 = L2_2.tConfirmCinematic
      else
        L4_2 = CHEAP_DECLINE
        if A0_2 == L4_2 then
          L3_2 = L2_2.tDeclineCinematic
        end
      end
      if L3_2 then
        L4_2 = MrxUtil.GetPrimaryCharacterName()
        L5_2 = L3_2.tCameraEffects
        if L5_2 then
          _ProcessCameraEffects(L2_2, L3_2.tCameraEffects[L4_2])
        end
        L6_2 = {}
        L6_2.sPositionObject = "Player1"
        L6_2.sPositionHardpoint = "Bone_Attach_Root"
        L6_2.sLookAtObject = "Player1"
        L6_2.sLookAtHardpoint = "Bone_Attach_Root"
        L6_2.bLookAtDirection = true
        _ProcessCameraSettings(L6_2)
        _NextCinematicFrame(L3_2[L4_2])
        goto lbl_75
        ::lbl_51::
        L2_2 = CHEAP_INTRO
        if A0_2 == L2_2 then
          L3_2 = WifBriefingData.GetIntroIdByIndex
          L4_2 = A1_2
          L3_2, L4_2, L5_2, L6_2, L7_2 = L3_2(L4_2)
          _PlayIntro(L3_2, L4_2, L5_2, L6_2, L7_2)
        else
          L2_2 = _CreateCheapCinematic(A0_2)
          if L2_2 then
            L3_2 = L2_2.tSequence
            if not L3_2 then
              L2_2.tSequence = {}
            end
            _ProcessCheapCinematic(L2_2, _StopClientCheapCinematic)
          end
        end
      end
    end
  end
  ::lbl_75::
end

NetSafePlayCheapCinematic = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _tViewedIntros
  return L0_2
end

GetViewedIntros = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L5_2 = A1_2
  MrxUtil.SetupLoadingCallback(_THIS, L5_2, A2_2)
  _nLoadPending = (_nLoadPending + 1)
  L3_2 = pairs
  L4_2 = A0_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L12_2 = {}
    L12_2[1] = _THIS
    _LoadTableOfAssets(L6_2, L7_2, MrxUtil.LoadingCallback, L12_2)
  end
  MrxUtil.LoadingCallback(_THIS)
end

LoadTableOfAssets = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L5_2 = A1_2
  MrxUtil.SetupLoadingCallback(_THIS, L5_2, A2_2)
  _nLoadPending = (_nLoadPending + 1)
  L3_2 = pairs
  L4_2 = A0_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L12_2 = {}
    L12_2[1] = _THIS
    _UnloadTableOfAssets(L6_2, L7_2, MrxUtil.LoadingCallback, L12_2)
  end
  MrxUtil.LoadingCallback(_THIS)
end

UnloadTableOfAssets = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  L4_2 = _tAssetLoadTimers
  if not L4_2 then
    L4_2 = {}
  end
  _tAssetLoadTimers = L4_2
  
  function L4_2(A0_3, A1_3)
    local L2_3, L3_3, L4_3, L5_3
    L2_3 = _tAssetLoadTimers[A0_2][A0_3]
    if L2_3 then
      Event.Delete(L2_3)
      L3_3 = _tAssetLoadTimers[A0_2]
      L3_3[A0_3] = nil
      if A1_3 then
      else
      end
      MrxUtil.CallWithOptionalArgs(A2_2, A3_2)
    else
    end
  end
  
  L5_2 = pairs
  L6_2 = A1_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = type(L8_2)
    L11_2 = type(L9_2)
    if L11_2 == "string" then
      L12_2 = L9_2
      _nLoadPending = (_nLoadPending + 1)
      L13_2 = _tAssetLoadTimers
      L14_2 = _tAssetLoadTimers[A0_2]
      if not L14_2 then
        L14_2 = {}
      end
      L13_2[A0_2] = L14_2
      L13_2 = _tAssetLoadTimers[A0_2]
      L16_2 = {}
      L16_2[1] = 15
      L16_2[2] = false
      L18_2 = {}
      L18_2[1] = L12_2
      L18_2[2] = true
      L13_2[L12_2] = Event.Create(Event.TimerRelative, L16_2, L4_2, L18_2)
      L18_2 = {}
      L18_2[1] = L12_2
      L18_2[2] = false
      _ProcessAsset(true, L12_2, A0_2, L4_2, L18_2)
    elseif L10_2 == "string" and L11_2 == "table" then
      L12_2 = L8_2
      L13_2 = L9_2
      L14_2 = MrxUtil.GetPrimaryCharacterName()
      if L12_2 == L14_2 or L12_2 == "MaleStarter" or L12_2 == "FemaleStarter" then
        _LoadTableOfAssets(A0_2, L13_2, A2_2, A3_2)
      end
    end
  end
end

_LoadTableOfAssets = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  L4_2 = _tAssetUnloadTimers
  if not L4_2 then
    L4_2 = {}
  end
  _tAssetUnloadTimers = L4_2
  
  function L4_2(A0_3, A1_3)
    local L2_3, L3_3, L4_3, L5_3
    L2_3 = _tAssetUnloadTimers[A0_2][A0_3]
    if L2_3 then
      Event.Delete(L2_3)
      L3_3 = _tAssetUnloadTimers[A0_2]
      L3_3[A0_3] = nil
      if A1_3 then
      else
      end
      MrxUtil.CallWithOptionalArgs(A2_2, A3_2)
    else
    end
  end
  
  L5_2 = pairs
  L6_2 = A1_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = type(L8_2)
    L11_2 = type(L9_2)
    if L11_2 == "string" then
      L12_2 = L9_2
      _nLoadPending = (_nLoadPending + 1)
      L13_2 = _tAssetUnloadTimers
      L14_2 = _tAssetUnloadTimers[A0_2]
      if not L14_2 then
        L14_2 = {}
      end
      L13_2[A0_2] = L14_2
      L13_2 = _tAssetUnloadTimers[A0_2]
      L16_2 = {}
      L16_2[1] = 15
      L16_2[2] = false
      L18_2 = {}
      L18_2[1] = L12_2
      L18_2[2] = true
      L13_2[L12_2] = Event.Create(Event.TimerRelative, L16_2, L4_2, L18_2)
      L18_2 = {}
      L18_2[1] = L12_2
      L18_2[2] = false
      _ProcessAsset(false, L12_2, A0_2, L4_2, L18_2)
    elseif L10_2 == "string" and L11_2 == "table" then
      L12_2 = L8_2
      L13_2 = L9_2
      L14_2 = MrxUtil.GetPrimaryCharacterName()
      if L12_2 == L14_2 or L12_2 == "MaleStarter" or L12_2 == "FemaleStarter" then
        _UnloadTableOfAssets(A0_2, L13_2, A2_2, A3_2)
      end
    end
  end
end

_UnloadTableOfAssets = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L6_2 = A1_2 .. "." .. A2_2
  L7_2 = _tLoadedAssets
  if not L7_2 then
    L7_2 = {}
  end
  _tLoadedAssets = L7_2
  if A0_2 then
    if A2_2 == "soundbank" or A2_2 == "wavebank" then
      L5_2 = MrxSoundBanks.LoadTempBank
    else
      L5_2 = Pg.LoadAsset
    end
    L7_2 = _tLoadedAssets[L6_2]
    if not L7_2 then
      L7_2 = 1
    else
      L7_2 = L7_2 + 1
    end
    L8_2 = _tLoadedAssets
    L8_2[L6_2] = L7_2
  else
    if A2_2 == "soundbank" or A2_2 == "wavebank" then
      L5_2 = MrxSoundBanks.UnloadTempBank
    else
      L5_2 = Pg.UnloadAsset
    end
    L7_2 = _tLoadedAssets[L6_2]
    if not L7_2 then
    else
      L7_2 = L7_2 - 1
      if L7_2 == 0 then
        L7_2 = nil
      end
      L8_2 = _tLoadedAssets
      L8_2[L6_2] = L7_2
    end
  end
  L5_2(A1_2, A2_2, A3_2, A4_2)
end

_ProcessAsset = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = 0
  L1_2 = pairs
  L2_2 = _tLoadedAssets
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L0_2 = L0_2 + 1
  end
  if L0_2 == 0 then
  end
  _tLoadedAssets = nil
end

_CheckAssets = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L0_2 = Player.GetProfileCostume()
  if not L0_2 then
    L0_2 = 0
  end
  L1_2 = WifPmcInterior.IsCostumeBriefingSafe((L0_2 + 1))
  if not L1_2 then
    _nOldOutfit = L0_2
    Player.SetProfileCostume(0)
    L1_2 = Player.GetPrimaryCharacter()
    L2_2 = MrxUtil.GetCharacterIdentity(L1_2)
    L3_2 = {}
    L3_2.mattias = "pmc_hum_mattias"
    L3_2.jennifer = "pmc_hum_jen"
    L3_2.chris = "pmc_hum_chris"
    Player.SetOutfit(L1_2, L3_2[L2_2])
    L6_2 = {}
    L6_2[1] = L1_2
    L6_2[2] = "awake"
    L6_2[3] = 0
    Event.Create(Event.ObjectIsReady, L6_2, Start2)
  else
    Start2()
  end
end

Start = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = _oStarter
  _tBriefings = L0_2.GetOfferedBriefings(L0_2)
  L0_2 = pairs
  L1_2 = _tBriefings
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L5_2 = WifBriefingData[L3_2]
    if not L5_2 then
      L5_2 = {}
    end
    L4_2.tConfig = L5_2
  end
  L0_2 = _oStarter
  L1_2 = L0_2
  L0_2 = L0_2.GetMissionsToBeAccepted
  L0_2, L1_2 = L0_2(L1_2)
  _sLastAcceptedMission = L1_2
  _tMissionsToBeAccepted = L0_2
  L0_2 = _tMissionsToBeAccepted
  if not L0_2 then
    _tMissionsToBeAccepted = {}
  end
  _nBaseShadowDistance = Graphics.GetShadowBaseDistance()
  Graphics.SetShadowBaseDistance(2)
  _SetDefaultCameraEffects()
  _bFadedIn = false
  MrxState.SetQuickFade(true)
  L1_2 = {}
  L1_2[1] = "Player1"
  L1_2[2] = "Starter"
  _SaveActorsOriginalPositions(L1_2)
  _AttachActorsToStartingLocations()
  _SetupPlayers(false)
  Gui.EnablePlayerMarkers(false)
  L0_2 = Hud.SubtitleBuffer
  L0_2.Clear(L0_2, {})
  L0_2 = Pda
  L2_2 = {}
  L2_2.bSuppress = true
  L0_2.SetSuppressed(L0_2, L2_2)
  L0_2 = _oStarter
  L0_2 = L0_2.GetFaction(L0_2)
  if L0_2 ~= "Pmc" then
    L1_2 = MrxFactionManager.GetAttitudeLabel(L0_2, "Pmc")
    if L1_2 == "Hostile" then
      MrxFactionManager.SetRelation(L0_2, "Pmc", -33)
    end
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_DISABLEMARKERS, {})
  end
  L1_2 = _oStarter
  L1_2 = L1_2.IsBoss(L1_2)
  if not L1_2 then
    L2_2 = GetActorGuid
    L3_2 = "Starter"
    L2_2, L3_2, L4_2, L5_2, L6_2 = L2_2(L3_2)
    Human.DisableWeapons(L2_2, L3_2, L4_2, L5_2, L6_2)
  end
  VO.Cancel()
  VO.SetCinematicMode(true)
  _BindFaceAnim(true, "Player1")
  L2_2 = true
  _BindFaceAnim(L2_2, "Starter")
  L1_2 = WifPmcInterior.GetWagerStatus
  L1_2, L2_2 = L1_2()
  if L1_2 then
    _WagerBegin(L1_2, L2_2)
    return
  end
  L4_2 = {}
  L4_2[1] = "Player1"
  L4_2[2] = "Starter"
  _SetActorsToDefaultPose(L4_2, false)
  L3_2 = _oStarter
  L3_2 = L3_2.IsBoss(L3_2)
  if L3_2 then
    _DisplayRootMenu()
  else
    L3_2 = _oStarter
    L3_2 = L3_2.IsPmcStarter(L3_2)
    if L3_2 then
      L3_2 = _HasSpecialCaseGreeting()
      if L3_2 then
        _SpecialCaseGreeting()
      else
        _ReturnToRootMenu()
      end
    else
      _Greeting()
    end
  end
end

Start2 = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = _CreateCheapCinematic(CHEAP_GREETING)
  if L0_2 then
    _ProcessCheapCinematic(L0_2, _BusinessCardMoment)
  else
    _BusinessCardMoment()
  end
end

_Greeting = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  _StopCheapCinematic()
  _DeleteSkipEvent()
  L0_2 = _oStarter
  L0_2 = L0_2.GetCardData(L0_2)
  if L0_2 then
    L1_2 = _oStarter
    L1_2 = L1_2.HasCardBeenDisplayed(L1_2)
    if not L1_2 then
      L1_2 = _oStarter
      L1_2.CardDisplayed(L1_2)
      L1_2 = Hud.CardFanfare
      L3_2 = {}
      L3_2.sFaction = L0_2.sFaction
      L3_2.sTitle = L0_2.sTitle
      L3_2.sName = L0_2.sName
      L3_2.sJobTitle = L0_2.sJobTitle
      L3_2.sPhone1 = L0_2.sPhone1
      L3_2.sPhone2 = L0_2.sPhone2
      L3_2.sEmail = ""
      L3_2.nDisplayTime = 3
      L3_2.fCallback = _JobRequest
      L1_2.Commence(L1_2, L3_2)
  end
  else
    L3_2 = {}
    L3_2[1] = 0
    Event.Create(Event.TimerRelative, L3_2, _JobRequest)
  end
end

_BusinessCardMoment = L0_1

function L0_1()
  local L0_2, L1_2
  _ReturnToRootMenu()
end

_JobRequest = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _oStarter
  L0_2 = L0_2.GetSpecialCaseGreeting(L0_2)
  if L0_2 then
    L0_2 = true
    return L0_2
  end
  L0_2 = false
  return L0_2
end

_HasSpecialCaseGreeting = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = _CreateCheapCinematic(CHEAP_SPECIALCASEGREETING)
  if L0_2 then
    _ProcessCheapCinematic(L0_2, _ReturnToRootMenu)
    return
  end
  _Fade(true)
  _DisplayRootMenu()
end

_SpecialCaseGreeting = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  _StopCheapCinematic()
  _DeleteSkipEvent()
  L0_2 = {}
  L1_2 = {}
  L2_2 = {}
  L3_2 = _oStarter
  L3_2 = L3_2.HasIntros(L3_2)
  if L3_2 then
    L3_2 = _oStarter
    L3_2 = L3_2.GetIntros(L3_2)
    L4_2 = pairs
    L5_2 = L3_2
    L4_2, L5_2, L6_2 = L4_2(L5_2)
    for L7_2, L8_2 in L4_2, L5_2, L6_2 do
      L9_2 = WifBriefingData.Intros[L7_2]
      if L9_2 then
        L10_2 = L9_2.sTitle
        if not L8_2 then
          L10_2 = "[yellow][shop.new] [white]" .. L10_2
        end
        table.insert(L0_2, L7_2)
        table.insert(L1_2, L10_2)
        table.insert(L2_2, _PlayIntro)
      end
    end
  end
  L3_2 = pairs
  L4_2 = _tBriefings
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L8_2 = table.getn(L0_2) + 1
    L9_2 = WifMissionData.IsMissionOnCriticalPath(L6_2)
    if L9_2 then
      L8_2 = 1
    end
    table.insert(L0_2, L8_2, L6_2)
    table.insert(L1_2, L8_2, ("\"" .. L7_2.sTitle .. "\""))
    table.insert(L2_2, L8_2, _BriefingSelected)
  end
  L3_2 = _oStarter
  L3_2 = L3_2.IsBoss(L3_2)
  if L3_2 then
    _BriefingSelected(L0_2[1])
    return
  end
  L3_2 = _oStarter
  L3_2 = L3_2.HasShop(L3_2)
  if L3_2 then
    L4_2 = _oStarter
    L3_2 = MrxFactionManager.GetPriceScale(L4_2.GetFaction(L4_2), "Pmc")
    if L3_2 then
      table.insert(L0_2, "Shop")
      table.insert(L1_2, "[Briefing.Shop]")
      table.insert(L2_2, _DisplayShop)
    end
  end
  L3_2 = _oStarter
  L3_2 = L3_2.HasTransitSystem(L3_2)
  if L3_2 then
    L3_2 = MrxTransit.GetNumValidLocations()
    if 0 < L3_2 then
      table.insert(L0_2, "Transit")
      table.insert(L1_2, "[Briefing.Transit]")
      table.insert(L2_2, _DisplayTransit)
    end
  end
  L3_2 = _oStarter
  L3_2 = L3_2.HasHintSystem(L3_2)
  if L3_2 then
    L4_2 = _oStarter
    L5_2 = L4_2
    L4_2 = L4_2.GetPmcName
    L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2 = L4_2(L5_2)
    L3_2 = WifHints.HasHint(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2)
    if L3_2 then
      table.insert(L0_2, "Hint")
      table.insert(L1_2, "[Briefing.Hint]")
      table.insert(L2_2, _DisplayHint)
    end
  end
  L3_2 = _oStarter
  L3_2 = L3_2.HasBribeSystem(L3_2)
  if L3_2 then
    _tBribableFactions = MrxFactionManager.GetBribableFactions()
    L3_2 = table.getn(_tBribableFactions)
    if 0 < L3_2 then
      table.insert(L0_2, "Bribe")
      table.insert(L1_2, "[Briefing.Bribe]")
      table.insert(L2_2, _DisplayBribes)
    end
  end
  table.insert(L0_2, "Cancel")
  table.insert(L1_2, "[Generic.Exit]")
  table.insert(L2_2, _Goodbye)
  L3_2 = Net.IsServer()
  if L3_2 then
    L3_2 = _ClientMenuBox
    if not L3_2 then
      Net.SendCustomEvent("MrxBriefing", NETEVENT_DISPLAYMENU, {})
      _ClientMenuBox = true
    end
  end
  L4_2 = Player.GetPrimaryPlayer()
  L5_2 = "[Briefing.RootMenuDialog]"
  L9_2 = {}
  L9_2[1] = L0_2
  L9_2[2] = L2_2
  L15_2 = table.getn
  L16_2 = L1_2
  L15_2, L16_2 = L15_2(L16_2)
  MrxGui.DisplayDialogBox(L4_2, L5_2, L1_2, 1, _RootMenuOptionSelected, L9_2, 48, 36, "left", "bottom", false, L15_2, L16_2)
end

_DisplayRootMenu = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  L3_2 = A1_2[A2_2]
  if L3_2 then
    L3_2(A0_2[A2_2])
  end
end

_RootMenuOptionSelected = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  _sSelectedMission = A0_2
  L1_2 = _oStarter
  L1_2 = L1_2.IsMissionAccepted(L1_2, _sSelectedMission)
  if L1_2 then
    _DisplayJobSummary()
  else
    L1_2 = WifMissionData.IsMissionAContract(_sSelectedMission)
    if L1_2 then
      L1_2 = false
      L2_2 = false
      L3_2 = nil
      L4_2 = MrxPlayState.IsFree()
      if not L4_2 then
        L1_2 = true
        L3_2 = "[Briefing.ContractActive]"
      end
      L4_2 = _oStarter
      L4_2 = L4_2.IsPmcStarter(L4_2)
      if L4_2 then
        L4_2 = WifPmcInterior.IsContractPending()
        if L4_2 then
          goto lbl_42
        end
      end
      L4_2 = _oStarter
      L4_2 = L4_2.IsContractPending(L4_2)
      ::lbl_42::
      if L4_2 then
        L2_2 = true
        L3_2 = "[Briefing.ContractPending]"
      end
      if L1_2 or L2_2 then
        L5_2 = Player.GetPrimaryPlayer()
        L7_2 = {}
        L7_2[1] = "[Generic.Back]"
        MrxGui.DisplayDialogBox(L5_2, L3_2, L7_2, 1, _DisplayRootMenu, {}, 48, 36, "left", "bottom", false, 1)
        return
      end
    end
    
    function L1_2()
      local L0_3, L1_3, L2_3
      L1_3 = _GetSelectedBriefingConfig().tPositions
      if L1_3 then
        _AttachActorsToHardpoints(L0_3.tPositions)
      end
      _FileLoaded()
    end
    
    _Fade(false, L1_2)
  end
end

_BriefingSelected = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L0_2 = Net.IsServer()
  if L0_2 then
    L0_2 = 1
    L1_2 = {}
    L2_2 = _sSelectedMission
    if L2_2 then
      L2_2 = WifBriefingData[_sSelectedMission]
      if L2_2 then
        L3_2 = L2_2.tActors
        if L3_2 then
          L4_2 = pairs
          L5_2 = L3_2
          L4_2, L5_2, L6_2 = L4_2(L5_2)
          for L7_2, L8_2 in L4_2, L5_2, L6_2 do
            if L7_2 ~= "Starter" then
              L1_2[L0_2] = GetActorGuid(L7_2)
              L0_2 = L0_2 + 1
            end
          end
        end
      end
    end
    Net.LoadMissionSpiel(WifMissionData.GetMissionIndexFromId(_sSelectedMission), L1_2)
  end
  dynamic_import(GetSpielFileName(_sSelectedMission), _FileLoaded)
end

_LoadSpiel = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2
  L1_2 = _GetSelectedBriefingConfig()
  MrxUtil.SetupLoadingCallback(_THIS, _StartSpiel)
  _nLoadPending = (_nLoadPending + 1)
  L2_2 = {}
  L2_2[1] = _THIS
  L3_2 = type(A0_2)
  if L3_2 == "table" then
    L3_2 = {}
    L3_2.tSequence = A0_2.tSequence
    L3_2.sParticipant1 = A0_2.sParticipant1
    L3_2.sParticipant2 = A0_2.sParticipant2
    L3_2.tParticipant1CamOverride = A0_2.tParticipant1CamOverride
    L3_2.tParticipant2CamOverride = A0_2.tParticipant2CamOverride
    L1_2.tCheapCinematic = L3_2
    L3_2 = ipairs
    L4_2 = A0_2.tSequence
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L8_2 = type(L7_2)
      if L8_2 == "table" then
        L8_2 = L7_2.sFlashFile
        if L8_2 then
          L8_2 = L7_2.sFlashFile
          L9_2 = L7_2.tPosition
          if L8_2 then
            _nLoadPending = (_nLoadPending + 1)
            _AddFlashObject(L8_2, L9_2, MrxUtil.LoadingCallback, L2_2)
          end
        end
      end
    end
  end
  L3_2 = L1_2.tAssetPreload
  if L3_2 then
    L3_2 = pairs
    L4_2 = L1_2.tAssetPreload
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      _LoadTableOfAssets(L6_2, L7_2, MrxUtil.LoadingCallback, L2_2)
    end
  end
  L3_2 = L1_2.tFaceAnimSets
  if L3_2 then
    L3_2 = pairs
    L4_2 = L1_2.tFaceAnimSets
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L8_2 = GetAnimSet(L7_2, L6_2)
      if L8_2 then
        _nLoadPending = (_nLoadPending + 1)
        _ProcessAsset(true, L8_2, "facefxanimationset", MrxUtil.LoadingCallback, L2_2)
      end
    end
  end
  L3_2 = Player.GetPrimaryCharacter()
  if L3_2 ~= nil then
    L4_2 = Object.IsAwake(L3_2)
    if not L4_2 then
      _nLoadPending = (_nLoadPending + 1)
      L6_2 = {}
      L6_2[1] = L3_2
      Event.Create(Event.ObjectIsReady, L6_2, MrxUtil.LoadingCallback, L2_2)
    end
  end
  L4_2 = Player.GetSecondaryCharacter()
  if L4_2 ~= nil then
    L5_2 = Object.IsAwake(L4_2)
    if not L5_2 then
      _nLoadPending = (_nLoadPending + 1)
      L7_2 = {}
      L7_2[1] = L4_2
      Event.Create(Event.ObjectIsReady, L7_2, MrxUtil.LoadingCallback, L2_2)
    end
  end
  L5_2 = L1_2.tCinematic
  if L5_2 then
    L5_2 = L1_2.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    L6_2 = ipairs
    L7_2 = L5_2
    L6_2, L7_2, L8_2 = L6_2(L7_2)
    for L9_2, L10_2 in L6_2, L7_2, L8_2 do
      L11_2 = L10_2.tAnims
      if L11_2 then
        L11_2 = pairs
        L12_2 = L10_2.tAnims
        L11_2, L12_2, L13_2 = L11_2(L12_2)
        for L14_2, L15_2 in L11_2, L12_2, L13_2 do
          _nLoadPending = (_nLoadPending + 1)
          _ProcessAsset(true, L15_2, "animation", MrxUtil.LoadingCallback, L2_2)
        end
      end
      L11_2 = L10_2.tFlash
      if L11_2 then
        L11_2 = L10_2.tFlash.sFile
        L12_2 = L10_2.tFlash.tPosition
        if L11_2 then
          _nLoadPending = (_nLoadPending + 1)
          _AddFlashObject(L11_2, L12_2, MrxUtil.LoadingCallback, L2_2)
        end
      end
    end
  end
  L6_2 = _sSelectedMission
  L6_2 = MrxRewardData.GetRewards(L6_2) or L6_2
  if L5_2 then
    L6_2 = L5_2.nWager
    if not L6_2 then
      L6_2 = L5_2.nWagerPercent
    end
  end
  L7_2 = Net.IsClient()
  if not L7_2 then
    L7_2 = _oStarter
    L7_2 = L7_2.IsPmcStarter(L7_2)
    if L7_2 and L6_2 then
      L1_2.tCheapCinematic = _CreateCheapCinematic(CHEAP_PMCWAGER)
      L7_2 = L1_2.tSlides
      if L7_2 then
        L1_2.tSlides = nil
      end
    end
  end
  MrxUtil.LoadingCallback(_THIS)
end

_FileLoaded = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L0_2 = Net.IsClient()
  if L0_2 then
    L0_2 = _tBriefings
    if not L0_2 then
      return
    end
    L1_2 = {}
    L2_2 = {}
    L2_2.sName = "OverTheShoulderLeft"
    L2_2.sBaseActor = "Starter"
    L2_2.sTargetActor = "Player1"
    L1_2.tShot = L2_2
    _ProcessCameraSettings(L1_2)
    _bNetSafeSpielLoaded = true
  end
  L0_2 = _GetSelectedBriefingConfig()
  _nCinematicFrame = nil
  _nSlideFrame = nil
  L1_2 = L0_2.tFaceAnimSets
  if L1_2 then
    L1_2 = pairs
    L2_2 = L0_2.tFaceAnimSets
    L1_2, L2_2, L3_2 = L1_2(L2_2)
    for L4_2, L5_2 in L1_2, L2_2, L3_2 do
      L6_2 = GetAnimSet(L5_2, L4_2)
      if L6_2 then
        _BindFaceAnim(false, L4_2)
        _BindFaceAnim(true, L4_2, L6_2)
      end
    end
  end
  L1_2 = nil
  L2_2 = L0_2.tCinematic
  if L2_2 then
    L1_2 = L0_2.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    L2_2 = nil
    L3_2 = L0_2.tCinematic.tCameraEffects
    if L3_2 then
      L2_2 = L0_2.tCinematic.tCameraEffects[MrxUtil.GetPrimaryCharacterName()]
      if L2_2 then
        _ProcessCameraEffects(L0_2, L2_2)
      end
    end
    L4_2 = {}
    L4_2.sPositionObject = "Player1"
    L4_2.sPositionHardpoint = "Bone_Attach_Root"
    L4_2.sLookAtObject = "Player1"
    L4_2.sLookAtHardpoint = "Bone_Attach_Root"
    L4_2.bLookAtDirection = true
    _ProcessCameraSettings(L4_2)
    L3_2 = Animation.SetUseBriefingLOD
    if L3_2 then
      Animation.SetUseBriefingLOD(true)
    end
  end
  
  function L2_2()
    local L0_3, L1_3, L2_3
    L0_3 = L0_2.tSlides
    if L0_3 then
      L0_3 = Net.IsClient()
      if not L0_3 then
        MrxCinematic.PlaceholderSequence(L0_2.tSlides, _CinematicComplete)
      end
    else
      L0_3 = L1_2
      if L0_3 then
        _NextCinematicFrame(L1_2, _CinematicComplete)
        _CreateSkipEvent(_CinematicComplete)
      else
        L0_3 = L0_2.tCheapCinematic
        if L0_3 then
          L0_3 = _oStarter
          L0_3 = L0_3.IsBoss(L0_3)
          if not L0_3 then
            L0_3 = _oStarter
            L0_3 = L0_3.IsPmcStarter(L0_3)
            if not L0_3 then
              goto lbl_53
            end
          end
          L0_3 = L0_2.tCheapCinematic
          L0_3.bFadeIn = not _bFadedIn
          _ProcessCheapCinematic(L0_2.tCheapCinematic, _CinematicComplete)
          goto lbl_69
          ::lbl_53::
          L0_3 = _bFadedIn
          if not L0_3 then
            _Fade(true)
          end
          _CinematicComplete()
        else
          L0_3 = Net.IsClient()
          if not L0_3 then
            _CinematicComplete()
          end
        end
      end
    end
    ::lbl_69::
  end
  
  if not L1_2 then
    L3_2 = L0_2.tCheapCinematic
    if not L3_2 then
      L3_2 = _bFadedIn
      if not L3_2 then
        _Fade(true, L2_2)
    end
  end
  else
    L2_2()
  end
end

_StartSpiel = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2
  L1_2 = _GetSelectedBriefingConfig().tCinematic
  if L1_2 then
    L2_2 = {}
    L2_2.sPositionObject = "HqInterior"
    L2_2.sPositionHardpoint = "hp_menu_camera"
    L2_2.sLookAtObject = "HqInterior"
    L2_2.sLookAtHardpoint = "hp_menu_camera"
    L2_2.bLookAtDirection = true
    _ProcessCameraSettings(L2_2)
  end
  L1_2 = Net.IsClient()
  if L1_2 then
    L1_2 = _ClientShopAvailable
    if not L1_2 then
      L1_2 = _ClientWaitBox
      if L1_2 then
        L1_2 = _ClientWaitBox
        L1_2.Close(L1_2)
        _ClientWaitBox = nil
      end
      L1_2 = MrxGui.DisplayDialogBox
      L2_2 = Player.GetLocalPlayer()
      L3_2 = "[Generic.WaitingForHostPlayer]"
      L4_2 = {}
      L5_2 = 1
      
      function L6_2()
        local L0_3, L1_3
        _ClientWaitBox = L0_3
      end
      
      _ClientWaitBox = L1_2(L2_2, L3_2, L4_2, L5_2, L6_2, {}, 48, 36, "left", "bottom", false)
    end
    return
  end
  L1_2 = MrxRewardData.GetRewards(_sSelectedMission)
  L2_2 = false
  if L1_2 then
    L3_2 = MrxRewardData.GetWagerData(L1_2)
    if L3_2 then
      L2_2 = true
      L4_2 = Hud.ResourceCounter
      L6_2 = {}
      L6_2.nDuration = -1
      L4_2.Show(L4_2, L6_2)
      L4_2 = L1_2.nWagered
      if not L4_2 then
        L1_2.nWagered = L3_2.nDefaultWager
      end
    end
  end
  L3_2 = WifMissionFlow.BuildMissionDescription(_sSelectedMission, false, false)
  L4_2 = DLC01_MissionHub.GetLeaderboardData
  L5_2 = _sSelectedMission
  L4_2, L5_2 = L4_2(L5_2)
  L6_2 = nil
  L7_2 = nil
  L8_2 = nil
  if L4_2 then
    L6_2 = L4_2.nRank
    L7_2 = true
    L8_2 = "[green][Scoring.PrevBest]: " .. MrxUtil.FormatMoney(L4_2.nScore) .. "[n][white]"
  end
  if L5_2 then
    L9_2 = nil
    L10_2 = Net.GetUserName
    if L10_2 then
      L9_2 = Net.GetUserName()
    end
    L8_2 = "[Scoring.Leaderboard]:[n]"
    L10_2 = table.sort
    L11_2 = L5_2
    
    function L12_2(A0_3, A1_3)
      local L2_3, L3_3
      L2_3 = A0_3.nScore > A1_3.nScore
      return L2_3
    end
    
    L10_2(L11_2, L12_2)
    L10_2 = nil
    if L9_2 then
      L11_2 = ipairs
      L12_2 = L5_2
      L11_2, L12_2, L13_2 = L11_2(L12_2)
      for L14_2, L15_2 in L11_2, L12_2, L13_2 do
        L16_2 = L15_2.sGamertag
        if L9_2 == L16_2 then
          L10_2 = L14_2
          L7_2 = true
          break
        end
      end
    end
    L11_2 = #L5_2
    L12_2 = nil
    L13_2 = nil
    if L11_2 < 5 then
      L12_2 = 1
      L13_2 = L11_2
    elseif L10_2 then
      if L10_2 < 3 then
        L12_2 = 1
        L13_2 = 5
      else
        L14_2 = L11_2 - 2
        if L10_2 > L14_2 then
          L12_2 = (L11_2 - 5) + 1
          L13_2 = L11_2
        else
          L12_2 = L10_2 - 2
          L13_2 = L10_2 + 2
        end
      end
    else
      L12_2 = 1
      L13_2 = 5
    end
    L14_2 = L12_2
    L15_2 = L13_2
    L16_2 = 1
    for L17_2 = L14_2, L15_2, L16_2 do
      L18_2 = L5_2[L17_2]
      L19_2 = ""
      L20_2 = ""
      if L17_2 == L10_2 then
        L19_2 = "[yellow]"
        L20_2 = "[white]"
      end
      L8_2 = L8_2 .. L19_2 .. L18_2.nDisplayRank .. ". " .. L18_2.sGamertag .. "[n]" .. L19_2 .. "[indent]" .. MrxUtil.FormatMoney(L18_2.nScore) .. "[n]" .. L20_2
    end
  end
  if L8_2 then
    if L7_2 then
      L3_2 = L8_2 .. "[n]" .. L3_2
    else
      L3_2 = L3_2 .. "[n]" .. L8_2
    end
  end
  MrxGuiDialogBox.DisplayScrollingDialogBox(Player.GetPrimaryPlayer(), L3_2, _HandleConfirmDialogInput, {}, L2_2)
end

_DisplayConfirmDialog = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  if A0_2 == 1 then
    L1_2 = WifRecommendationData.HasRecommendations(_sSelectedMission)
    if L1_2 then
      _DisplayRecommendationsDialog()
    else
      _AcceptOrDeclineMission(true)
    end
  elseif A0_2 == 2 then
    _AcceptOrDeclineMission(false)
  elseif A0_2 == 3 then
    _DisplayWagerDialog()
  end
end

_HandleConfirmDialogInput = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L0_2 = WifRecommendationData.GenerateRecommendationString
  L1_2 = _sSelectedMission
  L0_2, L1_2 = L0_2(L1_2)
  L2_2 = nil
  L3_2 = false
  if L1_2 == true then
    L4_2 = {}
    L4_2[1] = "Fiona.vo2fio22"
    L4_2[2] = "Fiona.aa3fio03"
    L2_2 = L4_2
  elseif L1_2 == false then
    L4_2 = {}
    L4_2[1] = "Fiona.Cam.63"
    L4_2[2] = "Fiona.Cam.64"
    L2_2 = L4_2
  end
  if L2_2 then
    Sound.CueSound(0, MrxUtil.GetRandomTableElement(L2_2))
  end
  if L0_2 then
    L0_2 = "[PDA.Map.RecommendationsHeader]\n" .. L0_2
  end
  L5_2 = Player.GetPrimaryPlayer()
  L8_2 = {}
  L8_2[1] = L3_2
  MrxGuiDialogBox.DisplayScrollingDialogBox(L5_2, L0_2, _HandleRecommendationsDialogInput, L8_2, false)
end

_DisplayRecommendationsDialog = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  if A0_2 then
    Sound.StopSound(0, A0_2)
  end
  if A1_2 == 1 then
    _AcceptOrDeclineMission(true)
  elseif A1_2 == 2 then
    _DisplayConfirmDialog()
  end
end

_HandleRecommendationsDialogInput = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = _GetSelectedBriefingConfig()
  L2_2 = nil
  L3_2 = Hud.ResourceCounter
  L3_2.Hide(L3_2, {})
  if A0_2 then
    L3_2 = WifMissionData.IsMissionAContract(_sSelectedMission)
    if L3_2 then
      L3_2 = _oStarter
      L3_2.SetPendingContract(L3_2, _sSelectedMission)
    end
    L3_2 = _oStarter
    L3_2.SetMissionAccepted(L3_2, _sSelectedMission, true)
    table.insert(_tMissionsToBeAccepted, _sSelectedMission)
    _sLastAcceptedMission = _sSelectedMission
  end
  L3_2 = _oStarter
  L3_2 = L3_2.IsBoss(L3_2)
  if not L3_2 then
    L3_2 = L1_2.nType
    L4_2 = WifBriefingData.knSimple
    if L3_2 ~= L4_2 then
      goto lbl_100
    end
  end
  L3_2 = nil
  L4_2 = _End
  if A0_2 then
    L5_2 = L1_2.tConfirmCinematic
    if L5_2 then
      L3_2 = L1_2.tConfirmCinematic
      L5_2 = Net.IsServer()
      if L5_2 then
        Net.SetBriefingCheapCinematic(CHEAP_CONFIRM)
      end
  end
  elseif not A0_2 then
    L5_2 = L1_2.tDeclineCinematic
    if L5_2 then
      L3_2 = L1_2.tDeclineCinematic
      L5_2 = Net.IsServer()
      if L5_2 then
        Net.SetBriefingCheapCinematic(CHEAP_DECLINE)
      end
    end
  end
  if L3_2 then
    L5_2 = MrxUtil.GetPrimaryCharacterName()
    L6_2 = L3_2.tCameraEffects
    if L6_2 then
      _ProcessCameraEffects(L1_2, L3_2.tCameraEffects[L5_2])
    end
    _NextCinematicFrame(L3_2[L5_2], L4_2)
    _CreateSkipEvent(L4_2)
  else
    L4_2()
    goto lbl_138
    ::lbl_100::
    L3_2 = _oStarter
    L3_2 = L3_2.IsPmcStarter(L3_2)
    if not L3_2 then
      L3_2 = _ReturnToRootMenu
      if A0_2 then
        L3_2 = _End
      end
      L4_2 = nil
      if A0_2 then
        L4_2 = _CreateCheapCinematic(CHEAP_JOBACCEPT)
      else
        L4_2 = _CreateCheapCinematic(CHEAP_JOBDECLINE)
      end
      if L4_2 then
        _ProcessCheapCinematic(L4_2, L3_2)
      else
        L3_2()
      end
    elseif A0_2 then
      _End()
    else
      _ReturnToRootMenu()
    end
  end
  ::lbl_138::
end

_AcceptOrDeclineMission = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  _StopCheapCinematic()
  _DeleteSkipEvent()
  _UnloadSpiel()
  _Fade(false, _PrepareForRootMenu)
end

_ReturnToRootMenu = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Hud.SubtitleBuffer
  L0_2.Clear(L0_2, {})
  L1_2 = {}
  L2_2 = {}
  L2_2.sName = "OverTheShoulderLeft"
  L2_2.sBaseActor = "Starter"
  L2_2.sTargetActor = "Player1"
  L1_2.tShot = L2_2
  _ProcessCameraSettings(L1_2)
  _SetDefaultCameraEffects()
  _AttachActorsToStartingLocations()
  L1_2 = {}
  L1_2[1] = "Player1"
  L1_2[2] = "Starter"
  _SetActorsToDefaultPose(L1_2)
  L2_2 = {}
  L2_2[1] = 0.5
  L4_2 = {}
  L4_2[1] = true
  L4_2[2] = _DisplayRootMenu
  Event.Create(Event.TimerRelative, L2_2, _Fade, L4_2)
end

_PrepareForRootMenu = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2
  L1_2 = _tBriefings[_sSelectedMission]
  L4_2 = MrxUtil.GetNumberOfDigits(MrxRewardData.GetWagerData(MrxRewardData.GetRewards(_sSelectedMission)).nWagerMax)
  L5_2 = 3
  L6_2 = MrxUtil.GetNumberOfDigits
  L7_2 = A0_2 or L7_2
  if not A0_2 then
    L7_2 = L3_2.nDefaultWager
  end
  L6_2 = L6_2(L7_2)
  L7_2 = L3_2.nWagerMin
  L8_2 = L3_2.nWagerMax
  L9_2 = L3_2.nCash
  if L8_2 > L9_2 then
    L9_2 = L3_2.nCash
    if L7_2 > L9_2 then
      L8_2 = L7_2
    else
      L8_2 = L3_2.nCash
    end
  end
  L9_2 = "[Briefing.ChangeWagerQuery]"
  L10_2 = ""
  L11_2 = L3_2.nWagerMin
  if L11_2 then
    L10_2 = L10_2 .. "[Briefing.MinWagerPrefix] " .. MrxUtil.FormatMoney(L3_2.nWagerMin)
  end
  L11_2 = L3_2.nWagerMax
  if L11_2 then
    L10_2 = L10_2 .. " [Briefing.MaxWagerPrefix] " .. MrxUtil.FormatMoney(L3_2.nWagerMax)
  end
  if L10_2 ~= "" then
    L10_2 = "(" .. L10_2 .. ")"
  else
    L10_2 = nil
  end
  L11_2 = "English"
  L12_2 = Sys.GetLanguage
  if L12_2 then
    L11_2 = Sys.GetLanguage() or L11_2
    if not L12_2 then
      L11_2 = "English"
    end
  end
  L12_2 = "$"
  L13_2 = ".0 [Generic.Money.Thousand]"
  if "English" == L11_2 then
    L12_2 = "$"
    L13_2 = ".0 [Generic.Money.Thousand]"
  elseif "French" == L11_2 then
    L12_2 = " "
    L13_2 = ",0 [Generic.Money.Thousand] $"
  elseif "German" == L11_2 then
    L12_2 = " "
    L13_2 = ",0 [Generic.Money.Thousand] $"
  elseif "Italian" == L11_2 then
    L12_2 = "$"
    L13_2 = ",0 [Generic.Money.Thousand]"
  elseif "Spanish" == L11_2 then
    L12_2 = " "
    L13_2 = ",0 [Generic.Money.Thousand] $"
  elseif "Russian" == L11_2 then
    L12_2 = "$"
    L13_2 = ",0 [Generic.Money.Thousand] $"
  end
  L14_2 = MrxGui.DisplayNumericBox
  L15_2 = Player.GetPrimaryPlayer()
  L16_2 = L9_2
  L17_2 = L10_2
  L18_2 = L12_2
  L19_2 = L13_2
  L20_2 = A0_2 or L20_2
  if not A0_2 then
    L20_2 = L3_2.nDefaultWager
  end
  L14_2(L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L7_2, L8_2, L6_2, L5_2, L4_2, _DisplayConfirmWagerDialog, {}, _DisplayCancelWagerDialog, {}, 48, 36, "right", "bottom", false)
end

_DisplayWagerDialog = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = MrxGui.DisplayDialogBox
  L2_2 = Player.GetPrimaryPlayer()
  L4_2 = [[
[Briefing.WagerConfirmDialog]
%s]]
  L5_2 = MrxUtil.FormatMoney
  L6_2 = A0_2
  L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2 = L5_2(L6_2)
  L3_2 = string.format(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2)
  L4_2 = {}
  L5_2 = "[Generic.Confirm]"
  L6_2 = "[Generic.Cancel]"
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L5_2 = 1
  
  function L6_2(A0_3, A1_3)
    local L2_3, L3_3
    if A1_3 == 1 then
      L2_3 = MrxRewardData.GetRewards(_sSelectedMission)
      if L2_3 then
        L2_3.nWagered = A0_3
      end
      _DisplayConfirmDialog()
    else
      _DisplayWagerDialog(A0_3)
    end
  end
  
  L7_2 = {}
  L7_2[1] = A0_2
  L1_2(L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, 48, 36, "right", "bottom", false, 2)
end

_DisplayConfirmWagerDialog = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = MrxGui.DisplayDialogBox
  L2_2 = Player.GetPrimaryPlayer()
  L3_2 = "[Briefing.WagerCancelDialog]"
  L4_2 = {}
  L5_2 = "[Generic.Yes]"
  L6_2 = "[Generic.No]"
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L5_2 = 2
  
  function L6_2(A0_3)
    local L1_3, L2_3
    if A0_3 == 1 then
      _DisplayConfirmDialog()
    else
      _DisplayWagerDialog(A0_2)
    end
  end
  
  L1_2(L2_2, L3_2, L4_2, L5_2, L6_2, {}, 48, 36, "right", "bottom", false, 2)
end

_DisplayCancelWagerDialog = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = Net.IsServer()
  if L0_2 then
    L0_2 = _ClientMenuBox
    if L0_2 then
      Net.SendCustomEvent("MrxBriefing", NETEVENT_HIDEMENU, {})
      _ClientMenuBox = nil
    end
  end
  L0_2 = _CreateCheapCinematic(CHEAP_GOODBYE)
  if L0_2 then
    _ProcessCheapCinematic(L0_2, _End)
  else
    _End()
  end
end

_Goodbye = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = Net.IsServer()
  if L0_2 then
    Net.SetLoadingScreen(true)
  end
  _StopCheapCinematic()
  _DeleteSkipEvent()
  _UnloadSpiel(true)
  L0_2 = _ClientMenuBox
  if L0_2 then
    L0_2 = Net.IsClient()
    if L0_2 then
      _CleanupClientMenu()
    else
      _ClientMenuBox = nil
    end
  end
  L0_2 = _ClientWaitBox
  if L0_2 then
    L0_2 = _ClientWaitBox
    L0_2.Close(L0_2)
    _ClientWaitBox = nil
  end
  _ClientMenuPending = nil
  MrxState.SetQuickFade(false)
  _Fade(false, _EndBegin)
end

_End = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L0_2 = Net.IsServer()
  if L0_2 then
    L0_2 = _nOldOutfit
    if L0_2 then
      Player.SetProfileCostume(_nOldOutfit)
      L0_2 = Player.GetPrimaryCharacter()
      Player.SetOutfit(L0_2, MrxPlayer.GetModelAtIndex(L0_2, _nOldOutfit))
      L5_2 = {}
      L5_2[1] = 1
      L5_2[2] = (_nOldOutfit + 1)
      Net.SendCustomEvent("WifPmcInterior", 2, L5_2)
      _nOldOutfit = nil
    end
  end
  Graphics.SetShadowBaseDistance(_nBaseShadowDistance)
  _SetCameraEffects(0, "RestoreAll", 1)
  _BindFaceAnim(false, "Player1")
  _BindFaceAnim(false, "Starter")
  _SetupPlayers(true)
  Gui.EnablePlayerMarkers(true)
  L0_2 = Pda
  L2_2 = {}
  L2_2.bSuppress = false
  L0_2.SetSuppressed(L0_2, L2_2)
  L0_2 = Net.IsServer()
  if L0_2 then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_ENABLEMARKERS, {})
  end
  L0_2 = Net.IsClient()
  if not L0_2 then
    L1_2 = {}
    L1_2[1] = "Player1"
    L1_2[2] = "Starter"
    _DetachActorsFromHardpoints(L1_2)
  else
    L1_2 = {}
    L1_2[1] = "Player1"
    _DetachActorsFromHardpoints(L1_2)
  end
  L0_2 = _oStarter
  L0_2 = L0_2.IsPmcStarter(L0_2)
  if L0_2 then
    L1_2 = {}
    L1_2[1] = "Player1"
    L1_2[2] = "Starter"
    _RestoreActorsToOriginalPositions(L1_2)
  end
  VO.SetCinematicMode(false)
  L0_2 = Animation.SetUseBriefingLOD
  if L0_2 then
    Animation.SetUseBriefingLOD(false)
  end
  L0_2 = Net.IsClient()
  if L0_2 then
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  else
    DLC01_MissionHub.BriefingComplete(_sLastAcceptedMission)
    L0_2 = _sLastAcceptedMission
    if L0_2 then
      DLC01_MissionHub.Exit(1, false)
    else
      MrxState.Exit(MrxState.STATE_WAITFORGAME)
    end
  end
  L0_2 = Net.IsServer()
  if L0_2 then
    Net.SetBriefingStarters(0)
  end
end

_EndBegin = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L1_2 = _tBriefings[_sSelectedMission].sTitle
  L2_2 = nil
  L3_2 = "\""
  L4_2 = L1_2
  L5_2 = [[
"

]]
  L2_2 = L3_2 .. L4_2 .. L5_2 .. "[" .. _sSelectedMission .. ".Terms.Summary]\n"
  L4_2 = Player.GetPrimaryPlayer()
  L6_2 = {}
  L6_2[1] = "[Generic.Back]"
  MrxGui.DisplayDialogBox(L4_2, L2_2, L6_2, 1, _DisplayRootMenu, {}, 48, 36, "left", "bottom", false, 1)
end

_DisplayJobSummary = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L0_2 = _oStarter
  if not L0_2 then
    _ClientMenuPending = true
    return
  end
  L0_2 = _ClientMenuPending
  if L0_2 then
    _ClientMenuPending = nil
  end
  L0_2 = _oStarter
  L0_2 = L0_2.IsBoss(L0_2)
  if L0_2 then
    return
  end
  L0_2 = _oStarter
  L0_2 = L0_2.HasShop(L0_2)
  if L0_2 then
    L1_2 = _oStarter
    L0_2 = MrxFactionManager.GetPriceScale(L1_2.GetFaction(L1_2), "Pmc")
    if L0_2 then
      _ClientShopAvailable = true
      L1_2 = Player.GetLocalPlayer()
      L2_2 = "[Briefing.RootMenuDialog]"
      L3_2 = {}
      L3_2[1] = "[Briefing.Shop]"
      _ClientMenuBox = MrxGui.DisplayDialogBox(L1_2, L2_2, L3_2, 1, _DisplayShop, {}, 48, 36, "left", "bottom", false)
    else
    end
  end
end

_DisplayClientMenu = L0_1

function L0_1()
  local L0_2, L1_2
  _ClientShopAvailable = L0_2
  MrxShop.Close()
  L0_2 = _ClientMenuBox
  if L0_2 then
    L0_2 = _ClientMenuBox
    L0_2.Close(L0_2)
    _ClientMenuBox = nil
  end
end

_CleanupClientMenu = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L1_2 = Net.IsClient()
  if L1_2 then
    L0_2 = _DisplayClientMenu
  else
    L0_2 = _DisplayRootMenu
  end
  MrxShop.Open(_oStarter, L0_2)
end

_DisplayShop = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  _ProcessCheapCinematic(_CreateCheapCinematic(CHEAP_HINT), _ReturnToRootMenu)
end

_DisplayHint = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L0_2 = {}
  L1_2 = ipairs
  L2_2 = _tBribableFactions
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    table.insert(L0_2, MrxFactionManager.GetPlayerVisibleName(L5_2))
  end
  table.insert(L0_2, "[Generic.Back]")
  L2_2 = Player.GetPrimaryPlayer()
  L3_2 = "[Briefing.BribeMenuDialog]"
  L13_2 = table.getn
  L14_2 = L0_2
  L13_2, L14_2 = L13_2(L14_2)
  MrxGui.DisplayDialogBox(L2_2, L3_2, L0_2, 1, _ConfirmBribe, {}, 48, 36, "left", "bottom", false, L13_2, L14_2)
end

_DisplayBribes = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L1_2 = table.getn(_tBribableFactions)
  if A0_2 > L1_2 then
    _DisplayRootMenu()
  else
    L1_2 = _tBribableFactions[A0_2]
    L2_2 = MrxFactionManager.GetPlayerVisibleName(L1_2)
    L4_2 = math.floor((MrxPmc.GetCashQty() * 0.2))
    L6_2 = "[Briefing.BribeConfirmQuery:" .. L2_2 .. "]" .. " (" .. MrxUtil.FormatMoney(L4_2) .. ")"
    L8_2 = Player.GetPrimaryPlayer()
    L10_2 = {}
    L11_2 = "[Generic.Confirm]"
    L12_2 = "[Generic.Cancel]"
    L10_2[1] = L11_2
    L10_2[2] = L12_2
    L13_2 = {}
    L13_2[1] = L1_2
    L13_2[2] = L4_2
    MrxGui.DisplayDialogBox(L8_2, L6_2, L10_2, 1, _ExecuteBribe, L13_2, 48, 36, "left", "bottom", false, 2)
  end
end

_ConfirmBribe = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2
  if A2_2 == 1 then
    MrxPmc.AddCashQty(-A1_2, nil, "[Generic.Bribes]")
    MrxFactionManager.SetRelation(A0_2, "Pmc", MrxFactionManager.GetAttitudeMedianValue("Friendly"))
    _DisplayRootMenu()
  else
    _DisplayRootMenu()
  end
end

_ExecuteBribe = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitCallback)
end

_DisplayTransit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  if not A1_2 then
    _DisplayRootMenu()
    return
  end
  _End()
  MrxPmc.AddFuelQty((-1 * math.min(MrxTransit.GetTransitFuelCost(), MrxPmc.GetFuelQty())))
  L2_2 = MrxSupportTransit
  L2_2.TransitInterfaceCallbackBriefing(L2_2, A0_2)
end

_TransitCallback = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L2_2 = GetActorGuid("Player1")
  L5_2 = _oStarter
  L5_2 = L5_2.GetPmcName(L5_2)
  L6_2 = " 1"
  L3_2 = Pg.GetGuidByName(("Wager Exit " .. L5_2 .. L6_2))
  L4_2 = Object.GetPosition
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  L7_2 = Object.GetYaw(L3_2)
  L8_2 = _tOriginalActorPositions
  L9_2 = {}
  L9_2[1] = L4_2
  L9_2[2] = L5_2
  L9_2[3] = L6_2
  L9_2[4] = L7_2
  L8_2[L2_2] = L9_2
  L8_2 = MrxRewardData.GetRewards(A0_2)
  if L8_2 then
    L9_2 = L8_2.nWagered
    if L9_2 then
      goto lbl_41
    end
  end
  _WagerEnd()
  do return end
  ::lbl_41::
  _nWager = L8_2.nWagered
  _sWagerMissionId = A0_2
  _bWagerWin = A1_2
  L9_2 = nil
  L10_2 = _bWagerWin
  if L10_2 then
    L9_2 = _CreateCheapCinematic(CHEAP_WAGERBEGINWIN)
  else
    L9_2 = _CreateCheapCinematic(CHEAP_WAGERBEGINLOSE)
  end
  _ProcessCheapCinematic(L9_2, _WagerTransaction)
end

_WagerBegin = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  _StopCheapCinematic()
  _DeleteSkipEvent()
  L2_2 = {}
  L2_2[1] = 0.1
  L4_2 = {}
  L4_2[1] = _bWagerWin
  L4_2[2] = 1
  Event.Create(Event.TimerRelative, L2_2, _ExecuteWagerTransaction, L4_2)
end

_WagerTransaction = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = false
  if A0_2 then
    MrxPmc.DisplayCash(MrxPmc.GetCashQty(), "[Generic.Wagers]", _nWager)
  elseif A1_2 == 1 then
    MrxPmc.DisplayCash(MrxPmc.GetCashQty(), "[Generic.Wagers]", -_nWager)
  else
    L3_2 = true
  end
  L4_2 = MrxRewardData.GetRewards(_sWagerMissionId)
  L4_2.nWagered = nil
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    L1_3 = A0_2
    if L1_3 then
      L0_3 = _CreateCheapCinematic(CHEAP_WAGERWON)
    else
      L1_3 = A1_2
      if L1_3 == 1 then
        L0_3 = _CreateCheapCinematic(CHEAP_WAGERLOST)
      else
        L0_3 = _CreateCheapCinematic(CHEAP_WAGERCHICKENSUIT)
      end
    end
    
    function L1_3()
      local L0_4, L1_4
      _StopCheapCinematic()
      _DeleteSkipEvent()
      _WagerEnd()
    end
    
    _ProcessCheapCinematic(L0_3, L1_3)
  end
  
  if L3_2 then
    WifPmcInterior.ChangeOutfit(Player.GetLocalCharacter(), "Chicken Suit", L5_2)
  else
    L5_2()
  end
end

_ExecuteWagerTransaction = L0_1

function L0_1()
  local L0_2, L1_2
  _nWager = L0_2
  _sWagerMissionId = nil
  _bWagerWin = nil
  _ReturnToRootMenu()
end

_WagerEnd = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = WifBriefingData.Intros[A0_2]
  if not L1_2 then
    L2_2 = Net.IsClient()
    if not L2_2 then
      _DisplayRootMenu()
    end
    return
  end
  L5_2 = {}
  L5_2[1] = A0_2
  _Fade(false, _StartIntro, L5_2)
end

_PlayIntro = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = WifBriefingData.Intros[A0_2]
  L2_2 = {}
  L2_2.sParticipant1 = "Player1"
  L2_2.sParticipant2 = "Starter"
  L2_2.bFadeIn = true
  L2_2.tSequence = L1_2.tSequence
  
  function L3_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    _ClearAllFlashObjects()
    L0_3 = Net.IsClient()
    if not L0_3 then
      L0_3 = _oStarter
      L0_3 = L0_3.HasViewedIntro(L0_3, A0_2)
      if not L0_3 then
        L0_3 = _tViewedIntros
        L0_3[A0_2] = true
      end
      L0_3 = _oStarter
      L0_3.SetViewedIntro(L0_3, A0_2, true, true)
      _ReturnToRootMenu()
    end
  end
  
  L4_2 = Net.IsServer()
  if L4_2 then
    L6_2 = WifBriefingData.GetIntroIndexById
    L7_2 = A0_2
    L6_2, L7_2 = L6_2(L7_2)
    Net.SetBriefingCheapCinematic(CHEAP_INTRO, L6_2, L7_2)
  end
  _ProcessCheapCinematic(L2_2, L3_2)
end

_StartIntro = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L1_2 = _sSelectedMission
  if not L1_2 then
    return
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    Net.UnloadMissionSpiel(A0_2)
  end
  L1_2 = Net.IsClient()
  if L1_2 and A0_2 then
    L2_2 = GetActorGuid
    L3_2 = "Player1"
    L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2 = L2_2(L3_2)
    Object.StopAllAnimation(L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2)
  end
  L2_2 = _GetSelectedBriefingConfig().tFaceAnimSets
  if L2_2 then
    L2_2 = pairs
    L3_2 = L1_2.tFaceAnimSets
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      L7_2 = GetAnimSet(L6_2, L5_2)
      if L7_2 then
        _BindFaceAnim(false, L5_2, L7_2)
        _BindFaceAnim(true, L5_2)
      end
    end
  end
  L2_2 = _nLoadPending
  if L2_2 == nil then
    MrxUtil.SetupLoadingCallback(_THIS)
  end
  _nLoadPending = (_nLoadPending + 1)
  L2_2 = L1_2.tAssetPreload
  if L2_2 then
    L2_2 = pairs
    L3_2 = L1_2.tAssetPreload
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      L11_2 = {}
      L11_2[1] = _THIS
      _UnloadTableOfAssets(L5_2, L6_2, MrxUtil.LoadingCallback, L11_2)
    end
  end
  MrxUtil.LoadingCallback(_THIS)
  L2_2 = L1_2.tFaceAnimSets
  if L2_2 then
    L2_2 = pairs
    L3_2 = L1_2.tFaceAnimSets
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      L7_2 = GetAnimSet(L6_2, L5_2)
      if L7_2 then
        _ProcessAsset(false, L7_2, "facefxanimationset")
      end
    end
  end
  L2_2 = L1_2.tCinematic
  if L2_2 then
    L2_2 = L1_2.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    L3_2 = ipairs
    L4_2 = L2_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L8_2 = L7_2.tAnims
      if L8_2 then
        L8_2 = pairs
        L9_2 = L7_2.tAnims
        L8_2, L9_2, L10_2 = L8_2(L9_2)
        for L11_2, L12_2 in L8_2, L9_2, L10_2 do
          L13_2 = GetActorGuid(L11_2)
          if L13_2 then
            L14_2 = Object.HasLabel(L13_2, "Human")
            if not L14_2 then
              Object.StopAllAnimation(L13_2)
            end
          end
          _ProcessAsset(false, L12_2, "animation")
        end
      end
    end
  end
  L2_2 = L1_2.tConfirmCinematic
  if L2_2 then
    _CleanupCinematic(L1_2.tConfirmCinematic)
  end
  L2_2 = L1_2.tDeclineCinematic
  if L2_2 then
    _CleanupCinematic(L1_2.tDeclineCinematic)
  end
  L2_2 = Net.IsClient()
  if L2_2 then
    L2_2 = L1_2.tCinematic
    if L2_2 then
      _CleanupCinematic(L1_2.tCinematic)
    end
  end
  dynamic_remove(GetSpielFileName(_sSelectedMission))
  _sSelectedMission = nil
end

_UnloadSpiel = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  L3_2 = _GetSelectedBriefingConfig()
  if not A0_2 then
    MrxUtil.CallWithOptionalArgs(A1_2, A2_2)
    return
  end
  L4_2 = false
  L5_2 = _nCinematicFrame
  if L5_2 then
    _nCinematicFrame = (_nCinematicFrame + 1)
  else
    _nCinematicFrame = 1
    L4_2 = true
  end
  L5_2 = A0_2[_nCinematicFrame]
  if not L5_2 then
    MrxUtil.CallWithOptionalArgs(A1_2, A2_2)
    return
  end
  L6_2 = A0_2[(_nCinematicFrame + 1)]
  L7_2 = L5_2.tAnims
  if L7_2 then
    if L4_2 == true then
      L9_2 = {}
      L9_2[1] = 0
      L9_2[2] = "CinematicStart"
      L11_2 = {}
      L11_2[1] = true
      Event.Create(Event.AnimationEvent, L9_2, _Fade, L11_2)
      L4_2 = nil
    end
    L7_2 = pairs
    L8_2 = L5_2.tAnims
    L7_2, L8_2, L9_2 = L7_2(L8_2)
    for L10_2, L11_2 in L7_2, L8_2, L9_2 do
      L13_2 = Object.HasLabel(GetActorGuid(L10_2), "Human")
      if L13_2 then
        Human.PlayRawAnimation(L12_2, L11_2, false, false, -1, true, true)
      else
        Object.PlayAnimation(L12_2, L11_2, false, nil, 0, true)
      end
    end
  end
  L7_2 = L5_2.tFlash
  if L7_2 then
    L7_2 = _tFlashTimers
    if not L7_2 then
      L7_2 = {}
    end
    _tFlashTimers = L7_2
    L7_2 = table.getn(_tFlashTimers)
    L8_2 = L5_2.tFlash.sFile
    if L8_2 ~= nil then
      _ShowFlashObject(L5_2.tFlash.sFile)
      L8_2 = _tFlashTimers
      L11_2 = {}
      L11_2[1] = L5_2.tFlash.nTime
      L13_2 = {}
      L13_2[1] = L5_2.tFlash.sFile
      L13_2[2] = L7_2
      L8_2[L7_2] = Event.Create(Event.TimerRelative, L11_2, _RemoveFlashObject, L13_2)
    else
      L8_2 = pairs
      L9_2 = L5_2.tFlash
      L8_2, L9_2, L10_2 = L8_2(L9_2)
      for L11_2, L12_2 in L8_2, L9_2, L10_2 do
        _ShowFlashObject(L12_2.sFile)
        L13_2 = _tFlashTimers
        L16_2 = {}
        L16_2[1] = L12_2.nTime
        L18_2 = {}
        L18_2[1] = L12_2.sFile
        L18_2[2] = L7_2
        L13_2[L7_2] = Event.Create(Event.TimerRelative, L16_2, _RemoveFlashObject, L18_2)
        L7_2 = L7_2 + 1
      end
    end
  end
  L7_2 = L5_2.tCamera
  if L7_2 then
    _ProcessCameraSettings(L5_2.tCamera)
  end
  if L4_2 == true then
    L7_2 = _bFadedIn
    if not L7_2 then
      _Fade(true)
    end
  end
  L7_2 = L5_2.OnTime
  if L7_2 then
    L7_2 = L5_2.OnTime
    L8_2 = L5_2.tAnims
    if not L8_2 then
      L8_2 = L5_2.tFlash
      if L8_2 then
        L8_2 = L6_2.tAnims
        if L8_2 then
          L7_2 = L7_2 - 1.8
        end
      end
    end
    
    function L8_2()
      local L0_3, L1_3, L2_3, L3_3
      L0_3 = L5_2
      L0_3._OnTimeEvent = nil
      L0_3 = _nCinematicFrame
      if L0_3 then
        _NextCinematicFrame(A0_2, A1_2, A2_2)
      else
      end
    end
    
    L11_2 = {}
    L11_2[1] = L5_2.OnTime
    L11_2[2] = false
    L5_2._OnTimeEvent = Event.Create(Event.TimerRelative, L11_2, L8_2)
  end
  L7_2 = L5_2.OnComplete
  if L7_2 then
    function L7_2(A0_3)
      local L1_3, L2_3, L3_3, L4_3
      
      if A0_3 == 1 then
        Event.Delete(L5_2._OnCompleteTransition)
      else
        Event.Delete(L5_2._OnCompleteEvent)
      end
      L1_3 = L5_2
      L1_3._OnCompleteTransition = nil
      L1_3 = L5_2
      L1_3._OnCompleteEvent = nil
      L1_3 = _nCinematicFrame
      if L1_3 then
        _NextCinematicFrame(A0_2, A1_2, A2_2)
      else
      end
    end
    
    L8_2 = GetActorGuid(L5_2.OnComplete)
    L11_2 = {}
    L11_2[1] = L8_2
    L11_2[2] = "CinematicEnd"
    L13_2 = {}
    L13_2[1] = 1
    L5_2._OnCompleteEvent = Event.Create(Event.AnimationEvent, L11_2, L7_2, L13_2)
    L11_2 = {}
    L11_2[1] = L8_2
    L11_2[2] = "*"
    L11_2[3] = "*"
    L11_2[4] = "complete"
    L13_2 = {}
    L13_2[1] = 2
    L5_2._OnCompleteTransition = Event.Create(Event.HumanStateTransition, L11_2, L7_2, L13_2)
  end
  L7_2 = L5_2.Stall
  if L7_2 == true then
  end
  L7_2 = L5_2.OnComplete
  if not L7_2 then
    L7_2 = L5_2.OnTime
    if not L7_2 then
      L7_2 = L5_2.Stall
      if L7_2 ~= true then
        _NextCinematicFrame(A0_2, A1_2, A2_2)
      end
    end
  end
end

_NextCinematicFrame = L0_1
L0_1 = {}
L0_1[1] = "hp_camera_a"
L0_1[2] = "hp_camera_b"
L0_1[3] = "hp_menu_camera"
L0_1[4] = "Bone_Attach_Root"
_tNetSafeHardpoints = L0_1
L0_1 = {}
L0_1[1] = "FaceToFace"
L0_1[2] = "OverTheShoulderRight"
L0_1[3] = "OverTheShoulderLeft"
L0_1[4] = "CloseUp"
_tNetSafeShotNames = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  if not A0_2 then
    L1_2 = nil
    return L1_2
  else
    L1_2 = 1
    L2_2 = ipairs
    L3_2 = _tNetSafeHardpoints
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      if L6_2 == A0_2 then
        return L1_2
      else
        L1_2 = L1_2 + 1
      end
    end
  end
  L1_2 = nil
  return L1_2
end

GetHardpointIndex = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  if not A0_2 then
    L1_2 = nil
    return L1_2
  else
    L1_2 = 1
    L2_2 = ipairs
    L3_2 = _tNetSafeShotNames
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      if L6_2 == A0_2 then
        return L1_2
      else
        L1_2 = L1_2 + 1
      end
    end
  end
  L1_2 = nil
  return L1_2
end

GetShotNameIndex = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = {}
  L1_2.nHold = 0
  L2_2 = A0_2.bHold
  if L2_2 ~= nil then
    L2_2 = A0_2.bHold
    if L2_2 == true then
      L1_2.nHold = 1
    else
      L1_2.nHold = 2
    end
  end
  L1_2.nBlendTime = A0_2.nBlendTime
  L1_2.tPosition = A0_2.tPosition
  L2_2 = A0_2.sPositionObject
  if L2_2 then
    L1_2.uPositionObject = GetActorGuid(A0_2.sPositionObject)
    L2_2 = L1_2.uPositionObject
    if not L2_2 then
    end
  end
  L1_2.nPositionHardpoint = GetHardpointIndex(A0_2.sPositionHardpoint)
  L1_2.tLookAt = A0_2.tLookAt
  L1_2.bLookAtDirection = A0_2.bLookAtDirection
  L2_2 = A0_2.sLookAtObject
  if L2_2 then
    L1_2.uLookAtObject = GetActorGuid(A0_2.sLookAtObject)
    L2_2 = L1_2.uLookAtObject
    if not L2_2 then
    end
  end
  L1_2.nLookAtHardpoint = GetHardpointIndex(A0_2.sLookAtHardpoint)
  L2_2 = A0_2.tShot
  if L2_2 then
    L1_2.tShot = {}
    L2_2 = L1_2.tShot
    L2_2.nName = GetShotNameIndex(A0_2.tShot.sName)
    L2_2 = L1_2.tShot
    L2_2.uBaseActor = GetActorGuid(A0_2.tShot.sBaseActor)
    L2_2 = L1_2.tShot
    L2_2.uTargetActor = GetActorGuid(A0_2.tShot.sTargetActor)
  end
  NetSafeProcessCameraSettings(L1_2)
end

_ProcessCameraSettings = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2
  A0_2.tPosition = A1_2
  A0_2.tLookAt = A2_2
  A0_2.tShot = A3_2
  NetSafeProcessCameraSettings(A0_2)
end

NetClientProcessCameraSettings = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L1_2 = Player.GetPrimaryPlayer()
  L2_2 = Player.GetSecondaryPlayer()
  L3_2 = nil
  L4_2 = nil
  if L1_2 then
    L3_2 = Player.GetCamera(L1_2)
  end
  if L2_2 then
    L4_2 = Player.GetCamera(L2_2)
  end
  L5_2 = {}
  L5_2[1] = L3_2
  L5_2[2] = L4_2
  L6_2 = A0_2.nPositionHardpoint
  if L6_2 then
    A0_2.sPositionHardpoint = _tNetSafeHardpoints[A0_2.nPositionHardpoint]
  end
  L6_2 = A0_2.nLookAtHardpoint
  if L6_2 then
    A0_2.sLookAtHardpoint = _tNetSafeHardpoints[A0_2.nLookAtHardpoint]
  end
  L6_2 = A0_2.nHold
  if L6_2 ~= 0 then
    L6_2 = A0_2.nHold
    if L6_2 == 1 then
      A0_2.bHold = true
    else
      A0_2.bHold = false
    end
  end
  L6_2 = ipairs
  L7_2 = L5_2
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    L11_2 = A0_2.bHold
    if L11_2 ~= nil then
      Camera.Hold(L10_2, A0_2.bHold, A0_2.bHold, true)
    end
    L11_2 = A0_2.nBlendTime
    if L11_2 then
      Camera.Blend(L10_2, A0_2.nBlendTime, true)
    end
    L11_2 = A0_2.tPosition
    if L11_2 then
      Camera.SetPosition(L10_2, A0_2.tPosition[1], A0_2.tPosition[2], A0_2.tPosition[3], true)
    else
      L11_2 = A0_2.uPositionObject
      if L11_2 then
        Camera.SetPosition(L10_2, A0_2.uPositionObject, A0_2.sPositionHardpoint, true)
      end
    end
    L11_2 = A0_2.tLookAt
    if L11_2 then
      Camera.SetLookAt(L10_2, A0_2.tLookAt[1], A0_2.tLookAt[2], A0_2.tLookAt[3], A0_2.bLookAtDirection, true)
    else
      L11_2 = A0_2.uLookAtObject
      if L11_2 then
        Camera.SetLookAt(L10_2, A0_2.uLookAtObject, A0_2.sLookAtHardpoint, A0_2.bLookAtDirection, true)
      end
    end
    L11_2 = A0_2.tShot
    if L11_2 then
      L11_2 = A0_2.tShot.nName
      if L11_2 then
        L11_2 = A0_2.tShot
        L11_2.sName = _tNetSafeShotNames[A0_2.tShot.nName]
      end
      Camera.SetShot(L10_2, A0_2.tShot.sName, A0_2.tShot.uBaseActor, A0_2.tShot.uTargetActor, true)
    end
  end
end

NetSafeProcessCameraSettings = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = CHEAP_GREETING
  if A0_2 == L2_2 then
    L2_2 = _GetGreeting
    L2_2, L3_2 = L2_2()
    if L2_2 then
      L4_2 = {}
      L4_2.sParticipant1 = "Player1"
      L4_2.sParticipant2 = "Starter"
      L4_2.bFadeIn = true
      L5_2 = {}
      L6_2 = {}
      L6_2.sSpeaker = "Starter"
      L6_2.sCue = L2_2
      L6_2.sAnim = L3_2
      L5_2[1] = L6_2
      L4_2.tSequence = L5_2
      L1_2 = L4_2
    else
      L4_2 = nil
      return L4_2
    end
  else
    L2_2 = CHEAP_SPECIALCASEGREETING
    if A0_2 == L2_2 then
      L2_2 = nil
      L3_2 = Net.IsClient()
      if L3_2 then
        L2_2 = WifPmcInterior.NetSafeGetSpecialCaseGreeting()
      else
        L3_2 = _oStarter
        L2_2 = L3_2.GetSpecialCaseGreeting(L3_2)
      end
      if L2_2 then
        L3_2 = {}
        L3_2.sParticipant1 = "Player1"
        L3_2.sParticipant2 = "Starter"
        L3_2.bFadeIn = true
        L4_2 = {}
        L5_2 = {}
        L5_2.sSpeaker = "Starter"
        L5_2.sCue = L2_2
        L4_2[1] = L5_2
        L3_2.tSequence = L4_2
        L1_2 = L3_2
      else
        L3_2 = nil
        return L3_2
      end
    else
      L2_2 = CHEAP_STARTINTRO
      if A0_2 == L2_2 then
      else
        L2_2 = CHEAP_JOBREQUEST
        if A0_2 == L2_2 then
          L2_2 = _GetJobRequest()
          if L2_2 then
            L3_2 = {}
            L3_2.Chris = "player_chris_job_briefing_greeting"
            L3_2.Jennifer = "player_jennifer_job_briefing_greeting"
            L3_2.Mattias = "player_mattias_job_briefing_greeting_fb"
            L4_2 = {}
            L4_2.sParticipant1 = "Player1"
            L4_2.sParticipant2 = "Starter"
            L5_2 = {}
            L6_2 = {}
            L6_2.sSpeaker = "Player1"
            L6_2.sCue = L2_2
            L6_2.sAnim = L3_2[MrxUtil.GetPrimaryCharacterName()]
            L5_2[1] = L6_2
            L4_2.tSequence = L5_2
            L1_2 = L4_2
          else
            L3_2 = nil
            return L3_2
          end
        else
          L2_2 = CHEAP_JOBACCEPT
          if A0_2 == L2_2 then
            L2_2 = _GetSpielResponse(true)
            if L2_2 then
              L3_2 = {}
              L3_2.Chris = "player_chris_job_briefing_yes"
              L3_2.Jennifer = "player_jennifer_job_briefing_yes"
              L3_2.Mattias = "player_mattias_job_briefing_yes_fb"
              L4_2 = L3_2[MrxUtil.GetPrimaryCharacterName()]
              L5_2 = {}
              L6_2 = {}
              L6_2.sSpeaker = "Player1"
              L6_2.sCue = L2_2
              L6_2.sAnim = L4_2
              L5_2[1] = L6_2
              L6_2 = {}
              L6_2.sParticipant1 = "Player1"
              L6_2.sParticipant2 = "Starter"
              L6_2.tSequence = L5_2
              L1_2 = L6_2
            else
              L3_2 = nil
              return L3_2
            end
          else
            L2_2 = CHEAP_JOBDECLINE
            if A0_2 == L2_2 then
              L2_2 = _GetSpielResponse(false)
              if L2_2 then
                L3_2 = {}
                L3_2.Chris = "player_chris_job_briefing_no"
                L3_2.Jennifer = "player_jennifer_job_briefing_no"
                L3_2.Mattias = "player_mattias_job_briefing_no_fb"
                L4_2 = L3_2[MrxUtil.GetPrimaryCharacterName()]
                L5_2 = {}
                L6_2 = {}
                L6_2.sSpeaker = "Player1"
                L6_2.sCue = L2_2
                L6_2.sAnim = L4_2
                L5_2[1] = L6_2
                L6_2 = {}
                L6_2.sParticipant1 = "Player1"
                L6_2.sParticipant2 = "Starter"
                L6_2.tSequence = L5_2
                L1_2 = L6_2
              else
                L3_2 = nil
                return L3_2
              end
            else
              L2_2 = CHEAP_WAGERBEGINWIN
              if A0_2 == L2_2 then
                L2_2 = {}
                L3_2 = {}
                L3_2[1] = "Eva.Wager.Lose01"
                L3_2[2] = "Eva.Wager.Lose02"
                L2_2.Eva = L3_2
                L3_2 = {}
                L3_2[1] = "Ewan.Wager.Lose01"
                L3_2[2] = "Ewan.Wager.Lose02"
                L2_2.Ewan = L3_2
                L3_2 = {}
                L3_2[1] = "Fiona.Wager.Lose01"
                L3_2[2] = "Fiona.Wager.Lose02"
                L2_2.Fiona = L3_2
                L3_2 = {}
                L3_2[1] = "Misha.Wager.Lose01"
                L3_2[2] = "Misha.Wager.Lose02"
                L2_2.Misha = L3_2
                L3_2 = _oStarter
                L4_2 = MrxUtil.GetRandomTableElement(L2_2[L3_2.GetPmcName(L3_2)])
                L5_2 = _GetGenericTalkBodyAnim("Starter")
                L6_2 = {}
                L6_2.sParticipant1 = "Player1"
                L6_2.sParticipant2 = "Starter"
                L6_2.bFadeIn = true
                L7_2 = {}
                L8_2 = {}
                L8_2.sSpeaker = "Starter"
                L8_2.sCue = L4_2
                L8_2.sAnim = L5_2
                L7_2[1] = L8_2
                L6_2.tSequence = L7_2
                L1_2 = L6_2
              else
                L2_2 = CHEAP_WAGERBEGINLOSE
                if A0_2 == L2_2 then
                  L2_2 = {}
                  L3_2 = {}
                  L3_2[1] = "Eva.Wager.Win01"
                  L3_2[2] = "Eva.Wager.Win02"
                  L2_2.Eva = L3_2
                  L3_2 = {}
                  L3_2[1] = "Ewan.Wager.Win01"
                  L3_2[2] = "Ewan.Wager.Win02"
                  L2_2.Ewan = L3_2
                  L3_2 = {}
                  L3_2[1] = "Fiona.Wager.Win01"
                  L3_2[2] = "Fiona.Wager.Win02"
                  L2_2.Fiona = L3_2
                  L3_2 = {}
                  L3_2[1] = "Misha.Wager.Win01"
                  L3_2[2] = "Misha.Wager.Win02"
                  L2_2.Misha = L3_2
                  L3_2 = _oStarter
                  L4_2 = MrxUtil.GetRandomTableElement(L2_2[L3_2.GetPmcName(L3_2)])
                  L5_2 = _GetGenericTalkBodyAnim("Starter")
                  L6_2 = {}
                  L6_2.sParticipant1 = "Player1"
                  L6_2.sParticipant2 = "Starter"
                  L6_2.bFadeIn = true
                  L7_2 = {}
                  L8_2 = {}
                  L8_2.sSpeaker = "Starter"
                  L8_2.sCue = L4_2
                  L8_2.sAnim = L5_2
                  L7_2[1] = L8_2
                  L6_2.tSequence = L7_2
                  L1_2 = L6_2
                else
                  L2_2 = CHEAP_WAGERWON
                  if A0_2 == L2_2 then
                    L2_2 = {}
                    L3_2 = {}
                    L3_2[1] = "Chris.Wager.Won.01"
                    L3_2[2] = "Chris.Wager.Won.02"
                    L2_2.Chris = L3_2
                    L3_2 = {}
                    L3_2[1] = "Jen.Wager.Won.01"
                    L3_2[2] = "Jen.Wager.Won.02"
                    L2_2.Jennifer = L3_2
                    L3_2 = {}
                    L3_2[1] = "Mattias.Wager.Won.01"
                    L3_2[2] = "Mattias.Wager.Won.02"
                    L2_2.Mattias = L3_2
                    L4_2 = MrxUtil.GetRandomTableElement(L2_2[MrxUtil.GetPrimaryCharacterName()])
                    L5_2 = _GetGenericTalkBodyAnim("Player1")
                    L6_2 = {}
                    L6_2.sParticipant1 = "Player1"
                    L6_2.sParticipant2 = "Starter"
                    L7_2 = {}
                    L8_2 = {}
                    L8_2.sSpeaker = "Player1"
                    L8_2.sCue = L4_2
                    L8_2.sAnim = L5_2
                    L7_2[1] = L8_2
                    L6_2.tSequence = L7_2
                    L1_2 = L6_2
                  else
                    L2_2 = CHEAP_WAGERLOST
                    if A0_2 == L2_2 then
                      L2_2 = {}
                      L3_2 = {}
                      L3_2[1] = "Chris.Wager.Lost.01"
                      L3_2[2] = "Chris.Wager.Lost.02"
                      L2_2.Chris = L3_2
                      L3_2 = {}
                      L3_2[1] = "Jen.Wager.Lost.01"
                      L3_2[2] = "Jen.Wager.Lost.02"
                      L2_2.Jennifer = L3_2
                      L3_2 = {}
                      L3_2[1] = "Mattias.Wager.Lost.01"
                      L3_2[2] = "Mattias.Wager.Lost.02"
                      L2_2.Mattias = L3_2
                      L4_2 = MrxUtil.GetRandomTableElement(L2_2[MrxUtil.GetPrimaryCharacterName()])
                      L5_2 = _GetGenericTalkBodyAnim("Player1")
                      L6_2 = {}
                      L6_2.sParticipant1 = "Player1"
                      L6_2.sParticipant2 = "Starter"
                      L7_2 = {}
                      L8_2 = {}
                      L8_2.sSpeaker = "Player1"
                      L8_2.sCue = L4_2
                      L8_2.sAnim = L5_2
                      L7_2[1] = L8_2
                      L6_2.tSequence = L7_2
                      L1_2 = L6_2
                    else
                      L2_2 = CHEAP_WAGERCHICKENSUIT
                      if A0_2 == L2_2 then
                        L2_2 = {}
                        L3_2 = {}
                        L3_2[1] = "Chris.CustomOutfit.Chicken.01"
                        L3_2[2] = "Chris.CustomOutfit.Chicken.02"
                        L3_2[3] = "Chris.CustomOutfit.Chicken.03"
                        L3_2[4] = "Chris.CustomOutfit.Chicken.04"
                        L3_2[5] = "Chris.CustomOutfit.Chicken.05"
                        L2_2.Chris = L3_2
                        L3_2 = {}
                        L3_2[1] = "Jen.CustomOutfit.Chicken.01"
                        L3_2[2] = "Jen.CustomOutfit.Chicken.02"
                        L3_2[3] = "Jen.CustomOutfit.Chicken.03"
                        L3_2[4] = "Jen.CustomOutfit.Chicken.04"
                        L3_2[5] = "Jen.CustomOutfit.Chicken.05"
                        L2_2.Jennifer = L3_2
                        L3_2 = {}
                        L3_2[1] = "Mattias.CustomOutfit.Chicken.01"
                        L3_2[2] = "Mattias.CustomOutfit.Chicken.02"
                        L3_2[3] = "Mattias.CustomOutfit.Chicken.03"
                        L3_2[4] = "Mattias.CustomOutfit.Chicken.04"
                        L3_2[5] = "Mattias.CustomOutfit.Chicken.05"
                        L2_2.Mattias = L3_2
                        L4_2 = MrxUtil.GetRandomTableElement(L2_2[MrxUtil.GetPrimaryCharacterName()])
                        L5_2 = nil
                        L6_2 = MrxUtil.GetPrimaryCharacterName()
                        if L6_2 == "Jennifer" then
                          L5_2 = "all_starter03_job_briefing_spiel"
                        else
                          L5_2 = "all_starter02_job_briefing_spiel"
                        end
                        L6_2 = {}
                        L6_2.sParticipant1 = "Player1"
                        L6_2.sParticipant2 = "Starter"
                        L7_2 = {}
                        L8_2 = {}
                        L8_2.sSpeaker = "Player1"
                        L8_2.sCue = L4_2
                        L8_2.sAnim = L5_2
                        L7_2[1] = L8_2
                        L6_2.tSequence = L7_2
                        L1_2 = L6_2
                      else
                        L2_2 = CHEAP_HINT
                        if A0_2 == L2_2 then
                          L3_2 = _oStarter
                          L4_2 = L3_2
                          L3_2 = L3_2.GetPmcName
                          L3_2, L4_2, L5_2, L6_2, L7_2, L8_2 = L3_2(L4_2)
                          L2_2 = WifHints.GetHint(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2)
                          L3_2 = {}
                          L3_2.sParticipant1 = "Player1"
                          L3_2.sParticipant2 = "Starter"
                          L4_2 = {}
                          L5_2 = {}
                          L5_2.sSpeaker = "Starter"
                          L5_2.sCue = L2_2
                          L4_2[1] = L5_2
                          L3_2.tSequence = L4_2
                          L1_2 = L3_2
                        else
                          L2_2 = CHEAP_GOODBYE
                          if A0_2 == L2_2 then
                            L2_2 = _GetGoodbye()
                            L3_2 = _oStarter
                            L3_2 = L3_2.IsPmcStarter(L3_2)
                            if not L3_2 and L2_2 then
                              L3_2 = {}
                              L4_2 = {}
                              L4_2.sSpeaker = "Starter"
                              L4_2.sCue = L2_2
                              L3_2[1] = L4_2
                              L4_2 = _oStarter
                              L4_2 = L4_2.IsMale(L4_2)
                              if L4_2 then
                                L4_2 = L3_2[1]
                                L4_2.sAnim = "all_starter02_job_briefing_goodbye"
                              else
                                L4_2 = L3_2[1]
                                L4_2.sAnim = "all_starter03_job_briefing_goodbye"
                              end
                              L4_2 = {}
                              L4_2.sParticipant1 = "Player1"
                              L4_2.sParticipant2 = "Starter"
                              L4_2.tSequence = L3_2
                              L1_2 = L4_2
                            else
                              L3_2 = nil
                              return L3_2
                            end
                          else
                            L2_2 = CHEAP_PMCWAGER
                            if A0_2 == L2_2 then
                              L2_2 = {}
                              L3_2 = {}
                              L3_2[1] = "Eva.Wager.OfferGeneric01"
                              L3_2[2] = "Eva.Wager.OfferGeneric02"
                              L2_2.Eva = L3_2
                              L3_2 = {}
                              L3_2[1] = "Ewan.Wager.OfferGeneric01"
                              L3_2[2] = "Ewan.Wager.OfferGeneric02"
                              L2_2.Ewan = L3_2
                              L3_2 = {}
                              L3_2[1] = "Fiona.Wager.OfferGeneric01"
                              L2_2.Fiona = L3_2
                              L3_2 = {}
                              L3_2[1] = "Misha.Wager.OfferGeneric01"
                              L2_2.Misha = L3_2
                              L3_2 = _oStarter
                              L4_2 = MrxUtil.GetRandomTableElement(L2_2[L3_2.GetPmcName(L3_2)])
                              L5_2 = _GetGenericTalkBodyAnim("Starter")
                              L6_2 = {}
                              L6_2.sParticipant1 = "Player1"
                              L6_2.sParticipant2 = "Starter"
                              L7_2 = {}
                              L8_2 = {}
                              L8_2.sSpeaker = "Starter"
                              L8_2.sCue = L4_2
                              L8_2.sAnim = L5_2
                              L7_2[1] = L8_2
                              L6_2.tSequence = L7_2
                              L1_2 = L6_2
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
        end
      end
    end
  end
  L2_2 = Net.IsServer()
  if L2_2 and L1_2 then
    Net.SetBriefingCheapCinematic(A0_2)
  end
  return L1_2
end

_CreateCheapCinematic = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2
  L3_2 = A0_2.sParticipant1
  L4_2 = A0_2.sParticipant2
  L5_2 = A0_2.bFadeIn
  A0_2.bFadeIn = nil
  _tAnimationEvents = {}
  
  function L6_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L1_3 = A0_3 == L3_2
    L2_3 = A0_3 == L4_2
    if not L1_3 and not L2_3 then
      L3_3 = false
      return L3_3
    end
    L3_3 = nil
    if L1_3 then
      L4_3 = A0_2.tParticipant1CamOverride
      if L4_3 then
        L3_3 = A0_2.tParticipant1CamOverride
    end
    else
      if L2_3 then
        L4_3 = A0_2.tParticipant2CamOverride
        if L4_3 then
          L3_3 = A0_2.tParticipant2CamOverride
      end
      else
        L4_3 = L3_2
        L5_3 = L4_2
        L6_3 = "OverTheShoulderLeft"
        if L2_3 then
          L4_3 = L4_2
          L5_3 = L3_2
          L6_3 = "OverTheShoulderRight"
        end
        L7_3 = {}
        L8_3 = {}
        L8_3.sName = L6_3
        L8_3.sBaseActor = L5_3
        L8_3.sTargetActor = L4_3
        L7_3.tShot = L8_3
        L3_3 = L7_3
      end
    end
    _ProcessCameraSettings(L3_3)
    L4_3 = true
    return L4_3
  end
  
  function L7_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3
    L1_3 = _GetGenericIdleBodyAnim(A0_3)
    L2_3 = GetActorGuid(A0_3)
    if L1_3 and L2_3 then
      Human.PlayRawAnimation(L2_3, L1_3, true, true, 0.5, false, true)
    end
    L3_3 = _tAnimationEvents
    if L3_3 then
      L3_3 = _tAnimationEvents[A0_3]
      if L3_3 then
        Event.Delete(_tAnimationEvents[A0_3])
        L3_3 = _tAnimationEvents
        L3_3[A0_3] = nil
      end
    end
  end
  
  function L8_2(A0_3, A1_3, A2_3)
    local L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3
    L3_3 = GetActorGuid(A0_3)
    if not L3_3 or not A1_3 then
      return
    end
    L4_3 = Human.PlayRawAnimation(L3_3, A1_3, A2_3, true, 0.5, false, true)
    if not A2_3 then
      L5_3 = _tAnimationEvents
      L8_3 = {}
      L8_3[1] = L3_3
      L8_3[2] = 0.1
      L10_3 = {}
      L10_3[1] = A0_3
      L5_3[A0_3] = Event.Create(Event.HumanAnimationNearlyCompleted, L8_3, L7_2, L10_3)
    end
  end
  
  function L9_2(A0_3, A1_3)
    local L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3
    L2_3 = _tFlashTimers
    if not L2_3 then
      L2_3 = {}
    end
    _tFlashTimers = L2_3
    L2_3 = table.getn(_tFlashTimers)
    _ShowFlashObject(A0_3)
    L3_3 = _tFlashTimers
    L6_3 = {}
    L6_3[1] = A1_3
    L8_3 = {}
    L8_3[1] = A0_3
    L8_3[2] = L2_3
    L3_3[L2_3] = Event.Create(Event.TimerRelative, L6_3, _RemoveFlashObject, L8_3)
  end
  
  L10_2 = MrxUtil.GetPrimaryCharacterName()
  L11_2 = {}
  
  function L12_2()
    local L0_3, L1_3, L2_3, L3_3
    
    function L0_3()
      local L0_4, L1_4, L2_4, L3_4, L4_4
      MrxVoSequence.Start(L11_2, true, MrxVoSequence.knPriorityBriefing, false)
      _CreateSkipEvent(A1_2, A2_2)
    end
    
    L1_3 = L5_2
    if L1_3 then
      _Fade(true, L0_3)
    else
      L0_3()
    end
  end
  
  L15_2 = L12_2
  MrxUtil.SetupLoadingCallback(_THIS, L15_2)
  _nLoadPending = (_nLoadPending + 1)
  L13_2 = ipairs
  L14_2 = A0_2.tSequence
  L13_2, L14_2, L15_2 = L13_2(L14_2)
  for L16_2, L17_2 in L13_2, L14_2, L15_2 do
    L18_2 = type(L17_2)
    if L18_2 == "table" then
      L18_2 = L17_2.sSpeaker
      if L18_2 then
        L18_2 = L17_2.sCue
        if L18_2 then
          L18_2 = L17_2.sSpeaker
          L19_2 = L17_2.sCue
          L20_2 = L17_2.sAnim
          if L18_2 == "Player1" then
            L21_2 = type(L19_2)
            if L21_2 == "table" then
              L19_2 = L19_2[L10_2]
            end
            L21_2 = type(L20_2)
            if L21_2 == "table" then
              L20_2 = L20_2[L10_2]
            end
          end
          if L16_2 == 1 then
            if L5_2 then
              L6_2(L18_2)
            else
              L23_2 = {}
              L25_2 = {}
              L25_2[1] = L18_2
              L23_2[1] = L6_2
              L23_2[2] = L25_2
              L23_2[3] = true
              table.insert(L11_2, L23_2)
            end
          end
          if L20_2 == nil or L20_2 == "" then
            L20_2 = _GetGenericTalkBodyAnim(L18_2)
          end
          L23_2 = {}
          L25_2 = {}
          L27_2 = L20_2
          L28_2 = true
          L25_2[1] = L18_2
          L25_2[2] = L27_2
          L25_2[3] = L28_2
          L23_2[1] = L8_2
          L23_2[2] = L25_2
          L23_2[3] = true
          table.insert(L11_2, L23_2)
          table.insert(L11_2, 0.1)
          L23_2 = {}
          L25_2 = GetActorGuid
          L26_2 = L18_2
          L25_2, L26_2, L27_2, L28_2, L29_2 = L25_2(L26_2)
          L23_2[1] = L19_2
          L23_2[2] = L25_2
          L23_2[3] = L26_2
          L23_2[4] = L27_2
          L23_2[5] = L28_2
          L23_2[6] = L29_2
          table.insert(L11_2, L23_2)
          L21_2 = nil
          L22_2 = L16_2 + 1
          while not L21_2 do
            L23_2 = A0_2.tSequence[L22_2]
            if L23_2 == nil then
              break
            else
              L24_2 = type(L23_2)
              if L24_2 == "table" then
                L21_2 = L23_2
              end
            end
            L22_2 = L22_2 + 1
          end
          L23_2 = nil
          if L21_2 then
            L23_2 = L21_2.sSpeaker
          end
          if L18_2 ~= L23_2 then
            L26_2 = {}
            L28_2 = {}
            L28_2[1] = L18_2
            L26_2[1] = L7_2
            L26_2[2] = L28_2
            L26_2[3] = true
            table.insert(L11_2, L26_2)
            if L23_2 then
              L26_2 = {}
              L28_2 = {}
              L28_2[1] = L23_2
              L26_2[1] = L6_2
              L26_2[2] = L28_2
              L26_2[3] = true
              table.insert(L11_2, L26_2)
              table.insert(L11_2, 0.1)
            else
            end
          end
      end
      else
        L18_2 = L17_2.sFlashFile
        if L18_2 then
          L18_2 = L17_2.nTime
          if L18_2 then
            _nLoadPending = (_nLoadPending + 1)
            L22_2 = {}
            L22_2[1] = _THIS
            _AddFlashObject(L17_2.sFlashFile, nil, MrxUtil.LoadingCallback, L22_2)
            L20_2 = {}
            L22_2 = {}
            L22_2[1] = L17_2.sFlashFile
            L22_2[2] = L17_2.nTime
            L20_2[1] = L9_2
            L20_2[2] = L22_2
            L20_2[3] = true
            table.insert(L11_2, L20_2)
          end
        end
      end
    else
      L18_2 = type(L17_2)
      if L18_2 == "number" then
        table.insert(L11_2, L17_2)
      end
    end
  end
  if A1_2 then
    L15_2 = {}
    L15_2[1] = A1_2
    L15_2[2] = A2_2
    L15_2[3] = true
    table.insert(L11_2, L15_2)
  end
  L7_2(L3_2)
  L7_2(L4_2)
  MrxUtil.LoadingCallback(_THIS)
end

_ProcessCheapCinematic = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Net.IsServer()
  if L0_2 then
    Net.SetBriefingCheapCinematic(0)
  end
  MrxVoSequence.Stop()
  L0_2 = _tAnimationEvents
  if L0_2 then
    L0_2 = pairs
    L1_2 = _tAnimationEvents
    L0_2, L1_2, L2_2 = L0_2(L1_2)
    for L3_2, L4_2 in L0_2, L1_2, L2_2 do
      Event.Delete(L4_2)
    end
    _tAnimationEvents = nil
  end
end

_StopCheapCinematic = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L4_2 = {}
  L5_2 = {}
  L5_2.sName = "CloseUp"
  L5_2.sBaseActor = "Player1"
  L5_2.sTargetActor = "Starter"
  L4_2.tShot = L5_2
  _ProcessCameraSettings(L4_2)
  L3_2 = ""
  L4_2 = #A0_2.tSequence
  L5_2 = ipairs
  L6_2 = A0_2.tSequence
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = type(L9_2)
    if L10_2 == "table" then
      L10_2 = L9_2.sSpeaker
      if L10_2 then
        L10_2 = L9_2.sCue
        if L10_2 then
          L10_2 = L9_2.sSpeaker
          L11_2 = L9_2.sCue
          if L10_2 == "Player1" then
            L12_2 = GetActorGuid(L10_2)
            if L12_2 then
              L10_2 = Object.GetLocalizedName(L12_2) or L10_2
              if not L13_2 then
              end
            end
          elseif L10_2 == "Starter" then
            L12_2 = _oStarter
            L10_2 = L12_2.GetPlayerVisibleName(L12_2) or L10_2
            if not L12_2 then
            end
          end
          L3_2 = L3_2 .. L10_2 .. ": [" .. L11_2 .. "]"
          if L8_2 < L4_2 then
            L12_2 = L3_2
            L13_2 = [[


]]
            L3_2 = L12_2 .. L13_2
          end
        end
      end
    end
  end
  L5_2 = MrxGui.DisplayDialogBox
  L6_2 = Player.GetPrimaryPlayer()
  L7_2 = L3_2
  L8_2 = {}
  L8_2[1] = "[Generic.Continue]"
  L9_2 = 1
  L10_2 = A1_2
  L11_2 = A2_2 or L11_2
  if not A2_2 then
    L11_2 = {}
  end
  L5_2(L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, 0, 0, "center", "center", false)
end

_ProcessCheapCinematicAsText = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = MrxUtil.GetPrimaryCharacterName()
  _ClearAllFlashObjects()
  _SetCameraEffects(0, "RestoreAll", 1)
  L2_2 = A0_2.tCameraEffects
  if L2_2 then
    L2_2 = A0_2.tCameraEffects[L1_2]
    L3_2 = ipairs
    L4_2 = L2_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L8_2 = L7_2._CameraEffectsTimer
      if L8_2 then
        Event.Delete(L7_2._CameraEffectsTimer)
        L7_2._CameraEffectsTimer = nil
      end
    end
  end
  L2_2 = A0_2[L1_2]
  if L2_2 then
    L3_2 = ipairs
    L4_2 = L2_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      L8_2 = L7_2._OnTimeEvent
      if L8_2 then
        Event.Delete(L7_2._OnTimeEvent)
        L7_2._OnTimeEvent = nil
      end
      L8_2 = L7_2._OnCompleteEvent
      if L8_2 then
        Event.Delete(L7_2._OnCompleteEvent)
        L7_2._OnCompleteEvent = nil
      end
      L8_2 = L7_2._OnCompleteTransition
      if L8_2 then
        Event.Delete(L7_2._OnCompleteTransition)
        L7_2._OnCompleteTransition = nil
      end
    end
  end
end

_CleanupCinematic = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  _StopCheapCinematic()
  _DeleteSkipEvent()
  L1_2 = {}
  L1_2.bHold = false
  _ProcessCameraSettings(L1_2)
  _nCinematicFrame = nil
  _nSlideFrame = nil
  _tSlide = nil
  L2_2 = {}
  L2_2[1] = 0.13
  L4_2 = {}
  L5_2 = {}
  L6_2 = {}
  L6_2.sName = "OverTheShoulderLeft"
  L6_2.sBaseActor = "Starter"
  L6_2.sTargetActor = "Player1"
  L5_2.tShot = L6_2
  L4_2[1] = L5_2
  Event.Create(Event.TimerRelative, L2_2, _ProcessCameraSettings, L4_2)
end

_StopClientCheapCinematic = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = _GetSelectedBriefingConfig().tCinematic
  if L1_2 then
    VO.CancelAll()
  else
    L1_2 = L0_2.tCheapCinematic
    if L1_2 then
      _StopCheapCinematic()
    end
  end
  _DeleteSkipEvent()
  L2_2 = {}
  L2_2.bHold = false
  _ProcessCameraSettings(L2_2)
  _nCinematicFrame = nil
  _nSlideFrame = nil
  _tSlide = nil
  L2_2 = {}
  L2_2[1] = "Player1"
  L2_2[2] = "Starter"
  _SetActorsToDefaultPose(L2_2, (L0_2.tCinematic == nil))
  L1_2 = L0_2.tCinematic
  if L1_2 then
    _CleanupCinematic(L0_2.tCinematic)
  else
    _ClearAllFlashObjects()
    L1_2 = nil
    L2_2 = L0_2.tCheapCinematic
    if L2_2 then
      L2_2 = L0_2.tCheapCinematic.sParticipant1
      if L2_2 == "Player1" then
        L1_2 = L0_2.tCheapCinematic.tParticipant1CamOverride
      else
        L2_2 = L0_2.tCheapCinematic.sParticipant2
        if L2_2 == "Player1" then
          L1_2 = L0_2.tCheapCinematic.tParticipant2CamOverride
        end
      end
    end
    if not L1_2 then
      L2_2 = {}
      L3_2 = {}
      L3_2.sName = "OverTheShoulderLeft"
      L3_2.sBaseActor = "Starter"
      L3_2.sTargetActor = "Player1"
      L2_2.tShot = L3_2
      L1_2 = L2_2
    end
    L4_2 = {}
    L4_2[1] = 0.13
    L6_2 = {}
    L6_2[1] = L1_2
    Event.Create(Event.TimerRelative, L4_2, _ProcessCameraSettings, L6_2)
  end
  _DisplayConfirmDialog()
end

_CinematicComplete = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = _tFlashObjects
  if not L1_2 then
    return
  end
  L1_2 = _tFlashObjects[A0_2]
  if not L1_2 then
    return
  end
  L1_2.SetVisible(L1_2, true)
  L1_2.Play(L1_2)
end

_ShowFlashObject = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L4_2 = _tFlashObjects
  if not L4_2 then
    _tFlashObjects = {}
  else
    L4_2 = _tFlashObjects[A0_2]
    if L4_2 then
      MrxUtil.CallWithOptionalArgs(A2_2, A3_2)
      return
    end
  end
  L4_2 = MrxGui.FlashWidget
  L4_2 = L4_2.new(L4_2)
  L4_2.SetIgnoresPause(L4_2, false)
  if A1_2 then
    L4_2.SetPosition(L4_2, A1_2[1], A1_2[2], A1_2[3], A1_2[4])
  else
    L4_2.SetFullscreen(L4_2, true)
  end
  L8_2 = A2_2
  L9_2 = A3_2
  L4_2.SetSwfFile(L4_2, A0_2, L8_2, L9_2)
  L7_2 = Player.GetLocalPlayer
  L7_2, L8_2, L9_2, L10_2 = L7_2()
  L4_2.SetOwner(L4_2, L7_2, L8_2, L9_2, L10_2)
  L4_2.SetVisible(L4_2, false)
  L4_2.Pause(L4_2)
  MrxGui.AddWidget(L4_2)
  L5_2 = _tFlashObjects
  L5_2[A0_2] = L4_2
end

_AddFlashObject = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = _tFlashObjects
  if not L2_2 then
    return
  end
  L2_2 = _tFlashObjects[A0_2]
  if not L2_2 then
    return
  end
  L3_2 = _tFlashObjects
  L3_2[A0_2] = nil
  if A1_2 then
    L3_2 = _tFlashTimers
    L3_2[A1_2] = nil
  end
  L2_2.SetSwfFile(L2_2, nil)
  MrxGui.RemoveWidget(L2_2)
  L2_2.Delete(L2_2)
end

_RemoveFlashObject = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = _tFlashObjects
  if not L0_2 then
    return
  end
  L0_2 = pairs
  L1_2 = _tFlashObjects
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    _RemoveFlashObject(L3_2)
  end
  L0_2 = _tFlashTimers
  if L0_2 then
    L0_2 = pairs
    L1_2 = _tFlashTimers
    L0_2, L1_2, L2_2 = L0_2(L1_2)
    for L3_2, L4_2 in L0_2, L1_2, L2_2 do
      Event.Delete(L4_2)
    end
    _tFlashTimers = nil
  end
  _tFlashObjects = nil
end

_ClearAllFlashObjects = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = Player.GetAllPlayers()
  L2_2 = ipairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    if A0_2 then
    else
    end
    Player.SetCinematicMode(L6_2, not A0_2, "Bone_Attach_Root", 0, true)
    L8_2 = Human.IsCarrying(Player.GetCharacter(L6_2))
    if L8_2 then
      Human.Drop(L7_2)
    end
    Human.Scrub(L7_2)
    if A0_2 then
      L8_2 = Vehicle.GetFromRider(L7_2)
      if not L8_2 then
        Human.SetState(L7_2, "Upright", "Idle")
      end
    end
    Human.SetJostleEnabled(L7_2, A0_2)
  end
end

_SetupPlayers = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2
  if not A2_2 then
    if A1_2 == "Player1" then
      A2_2 = "Global_Job_Briefing_" .. MrxUtil.GetPrimaryCharacterName()
    else
      L3_2 = _oStarter
      A2_2 = L3_2.GetGlobalFaceFxSet(L3_2)
      if not A2_2 then
        L3_2 = false
        return L3_2
      end
    end
  end
  L3_2 = nil
  if A0_2 then
    L3_2 = Animation.BindFaceAnimSet
  else
    L3_2 = Animation.UnbindFaceAnimSet
  end
  L5_2 = L3_2(GetActorGuid(A1_2), A2_2)
  if not L5_2 then
  end
  return L5_2
end

_BindFaceAnim = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = GetActorGuid("HqInterior")
  if not L1_2 then
    return
  end
  L2_2 = pairs
  L3_2 = A0_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = GetActorGuid(L5_2)
    Object.DisablePhysics(L7_2)
    Object.Attach(L1_2, L6_2, L7_2)
    Object.SetTransformToObject(L7_2, L1_2, L6_2)
    Human.PersistTransform(L7_2)
  end
end

_AttachActorsToHardpoints = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = GetActorGuid("HqInterior")
  if not L1_2 then
    return
  end
  L2_2 = ipairs
  L3_2 = A0_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = GetActorGuid(L6_2)
    Object.Detach(L1_2, L7_2)
    Object.EnablePhysics(L7_2)
  end
end

_DetachActorsFromHardpoints = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = Pg.GetGuidByName("HqInterior")
  L2_2 = pairs
  L3_2 = A0_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = GetActorGuid(L5_2)
    Object.Detach(L1_2, L7_2)
    L10_2 = Pg.GetGuidByName
    L11_2 = L6_2
    L10_2, L11_2 = L10_2(L11_2)
    Object.SetTransformToObject(L7_2, L10_2, L11_2)
    Human.PersistTransform(L7_2)
    Object.DisablePhysics(L7_2)
  end
end

_AttachActorsToLocations = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L0_2 = _oStarter
  L0_2 = L0_2.IsPmcStarter(L0_2)
  if L0_2 then
    L0_2 = _oStarter
    L1_2 = WifPmcInterior.GetStarterBriefingLocs(L0_2.GetName(L0_2))
    L3_2 = {}
    L3_2.Player1 = L1_2[2]
    L3_2.Starter = L1_2[1]
    _AttachActorsToLocations(L3_2)
  else
    L1_2 = {}
    L1_2.Player1 = "hp_playerA"
    L1_2.Starter = "hp_starter"
    _AttachActorsToHardpoints(L1_2)
  end
end

_AttachActorsToStartingLocations = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L1_2 = _tOriginalActorPositions
  if not L1_2 then
    _tOriginalActorPositions = {}
  end
  L1_2 = ipairs
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = GetActorGuid(L5_2)
    L7_2 = Object.GetPosition
    L8_2 = L6_2
    L7_2, L8_2, L9_2 = L7_2(L8_2)
    L10_2 = Object.GetYaw(L6_2)
    L11_2 = _tOriginalActorPositions
    L12_2 = {}
    L12_2[1] = L7_2
    L12_2[2] = L8_2
    L12_2[3] = L9_2
    L12_2[4] = L10_2
    L11_2[L6_2] = L12_2
  end
end

_SaveActorsOriginalPositions = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L1_2 = _tOriginalActorPositions
  if not L1_2 then
    return
  end
  L1_2 = ipairs
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L7_2 = _tOriginalActorPositions[GetActorGuid(L5_2)]
    if L7_2 then
      L7_2 = unpack
      L8_2 = _tOriginalActorPositions[L6_2]
      L7_2, L8_2, L9_2, L10_2 = L7_2(L8_2)
      Object.SetPosition(L6_2, L7_2, L8_2, L9_2)
      Object.SetYaw(L6_2, L10_2)
      Object.EnablePhysics(L6_2)
    end
  end
  _tOriginalActorPositions = nil
end

_RestoreActorsToOriginalPositions = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L2_2 = 0.5
  if A1_2 == false then
    L2_2 = -1
  end
  L3_2 = ipairs
  L4_2 = A0_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    Human.PlayRawAnimation(GetActorGuid(L7_2), _GetGenericIdleBodyAnim(L7_2), true, A1_2, L2_2, false, true)
  end
end

_SetActorsToDefaultPose = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  if A0_2 == "Player1" then
    L1_2 = Player.GetPrimaryCharacter
    return L1_2()
  elseif A0_2 == "Player2" then
    L1_2 = Player.GetSecondaryCharacter
    return L1_2()
  elseif A0_2 == "Starter" then
    L1_2 = _oStarter
    L2_2 = L1_2
    L1_2 = L1_2.GetActor
    return L1_2(L2_2)
  end
  L1_2 = Pg.GetGuidByName
  L2_2 = A0_2
  return L1_2(L2_2)
end

GetActorGuid = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = MrxUtil.GetPrimaryCharacterName()
  if A0_2 == "ChiCon009" then
    L2_2 = "Spiel_Job_Chi09_" .. L1_2
    return L2_2
  elseif A0_2 == "OilCon020" then
    L2_2 = "Spiel_Job_Oil00_" .. L1_2
    return L2_2
  elseif A0_2 == "OilCon050" then
    L2_2 = "Spiel_Job_Oil01_" .. L1_2
    return L2_2
  end
  L2_2 = MrxUtil.ExplodeMissionName
  L3_2 = A0_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  L5_2 = "Job"
  if L3_2 then
    L5_2 = "MinorContract"
  end
  L6_2 = "Spiel_" .. L5_2 .. "_" .. L2_2 .. string.format("%02d", L4_2) .. "_" .. L1_2
  return L6_2
end

GetSpielFileName = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2
  if A1_2 == "Player1" then
    L3_2 = MrxUtil.GetPrimaryCharacterName()
    L4_2 = type(A0_2)
    if L4_2 == "table" then
      L4_2 = A0_2[L3_2]
      if L4_2 then
        L2_2 = A0_2[L3_2]
    end
    else
      L2_2 = nil
    end
  end
  return L2_2
end

GetAnimSet = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = _oStarter
  L0_2 = MrxFactionManager.GetAttitudeLabel(L1_2.GetFaction(L1_2), "Pmc")
  L1_2 = {}
  L1_2.Neutral = "Neutral"
  L1_2.Friendly = "Positive"
  L2_2 = L1_2[L0_2]
  if not L2_2 then
    return
  end
  L3_2 = _tBriefingWrapper
  if not L3_2 then
    return
  end
  L3_2 = _tBriefingWrapper.Greetings
  if not L3_2 then
    return
  end
  L4_2 = _oStarter
  L4_2 = not L4_2.HasCardBeenDisplayed(L4_2)
  if L4_2 then
    L3_2 = L3_2.Initial
  else
    L3_2 = L3_2.Subsequent
  end
  if not L3_2 then
    return
  end
  L3_2 = L3_2[L2_2]
  if not L3_2 then
    return
  end
  L5_2 = nil
  if L4_2 then
    L5_2 = L3_2[MrxUtil.GetPrimaryCharacterName()]
  else
    L5_2 = MrxUtil.GetRandomTableElement(L3_2)
  end
  L6_2 = nil
  L7_2 = _oStarter
  L7_2 = L7_2.IsMale(L7_2)
  if L7_2 then
    if L2_2 == "Negative" then
      L6_2 = "all_starter02_job_briefing_greeting_angry"
    elseif L2_2 == "Neutral" then
      L6_2 = "all_starter02_job_briefing_greeting_neutral"
    elseif L2_2 == "Positive" then
      L6_2 = "all_starter02_job_briefing_greeting_happy"
    end
  elseif L2_2 == "Negative" then
    L6_2 = "all_starter03_job_briefing_greeting_angry"
  elseif L2_2 == "Neutral" then
    L6_2 = "all_starter03_job_briefing_greeting_neutral"
  elseif L2_2 == "Positive" then
    L6_2 = "all_starter03_job_briefing_greeting_happy"
  end
  L7_2 = L5_2
  L8_2 = L6_2
  return L7_2, L8_2
end

_GetGreeting = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = _tHeroWrapperVo.tJobRequest[MrxUtil.GetPrimaryCharacterName()]
  L1_2 = MrxUtil.GetRandomTableElement
  L2_2 = L0_2
  return L1_2(L2_2)
end

_GetJobRequest = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  if A0_2 then
    L1_2 = "tJobAccept"
  else
    L1_2 = "tJobDecline"
  end
  L2_2 = _tHeroWrapperVo[L1_2][MrxUtil.GetPrimaryCharacterName()]
  L3_2 = MrxUtil.GetRandomTableElement
  L4_2 = L2_2
  return L3_2(L4_2)
end

_GetSpielResponse = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = _tBriefingWrapper
  if not L0_2 then
    return
  end
  L0_2 = _tBriefingWrapper.Goodbyes
  if L0_2 then
    L1_2 = MrxUtil.GetRandomTableElement
    L2_2 = L0_2
    return L1_2(L2_2)
  end
end

_GetGoodbye = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = {}
  if A0_2 == "Starter" then
    L2_2 = _oStarter
    L2_2 = L2_2.IsMale(L2_2)
    if L2_2 then
      L1_2[1] = "all_starter02_job_briefing_spiel"
    else
      L1_2[1] = "all_starter03_job_briefing_spiel"
    end
  else
    L2_2 = MrxUtil.GetPrimaryCharacterName()
    L3_2 = {}
    L3_2.Chris = "player_chris_job_briefing_spiel"
    L3_2.Jennifer = "player_jennifer_job_briefing_spiel"
    L3_2.Mattias = "player_mattias_job_briefing_spiel_fb"
    L1_2[1] = L3_2[L2_2]
  end
  L2_2 = MrxUtil.GetRandomTableElement
  L3_2 = L1_2
  return L2_2(L3_2)
end

_GetGenericTalkBodyAnim = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = {}
  if A0_2 == "Starter" then
    L2_2 = _oStarter
    L2_2 = L2_2.IsMale(L2_2)
    if L2_2 then
      L1_2[1] = "all_starter02_job_briefing_idle"
    else
      L1_2[1] = "all_starter03_job_briefing_idle"
    end
  else
    L2_2 = MrxUtil.GetPrimaryCharacterName()
    L3_2 = {}
    L3_2.Chris = "player_chris_job_briefing_idle"
    L3_2.Jennifer = "player_jennifer_job_briefing_idle"
    L3_2.Mattias = "player_mattias_job_briefing_idle_fb"
    L1_2[1] = L3_2[L2_2]
  end
  L2_2 = MrxUtil.GetRandomTableElement
  L3_2 = L1_2
  return L2_2(L3_2)
end

_GetGenericIdleBodyAnim = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _tBriefings
  if L0_2 then
    L0_2 = _sSelectedMission
    if L0_2 then
      L0_2 = _tBriefings[_sSelectedMission]
      if L0_2 then
        goto lbl_14
      end
    end
  end
  L0_2 = nil
  do return L0_2 end
  ::lbl_14::
  L0_2 = _tBriefings[_sSelectedMission].tConfig
  return L0_2
end

_GetSelectedBriefingConfig = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = Net.IsClient()
  if L2_2 then
    return
  end
  L2_2 = "selection"
  L3_2 = Sys.IsConfirmOnCircle()
  if L3_2 then
    L2_2 = "cancel"
  end
  
  function L3_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3
    L0_3 = Event.CreatePersistent
    L1_3 = Event.Button
    L2_3 = {}
    L2_3[1] = Player.GetLocalPlayer()
    L2_3[2] = L2_2
    L2_3[3] = "press"
    L2_3[4] = true
    
    function L3_3()
      local L0_4, L1_4, L2_4
      L0_4 = Hud.SubtitleBuffer
      L0_4.Clear(L0_4, {})
      MrxUtil.CallWithOptionalArgs(A0_2, A1_2)
    end
    
    _uButtonEvent = L0_3(L1_3, L2_3, L3_3)
  end
  
  L6_2 = {}
  L6_2[1] = 0.5
  _uButtonDelayEvent = Event.Create(Event.TimerRelative, L6_2, L3_2)
end

_CreateSkipEvent = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _uButtonDelayEvent
  if L0_2 then
    Event.Delete(_uButtonDelayEvent)
    _uButtonDelayEvent = nil
  end
  L0_2 = _uButtonEvent
  if L0_2 then
    Event.Delete(_uButtonEvent)
    _uButtonEvent = nil
  end
end

_DeleteSkipEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = pairs
  L3_2 = A1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = Event.Create
    L8_2 = Event.TimerRelative
    L9_2 = {}
    L9_2[1] = L6_2.nTime
    L9_2[2] = false
    
    function L10_2()
      local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3
      L0_3 = type(L6_2.tDepthOfField)
      if L0_3 == "table" then
        L0_3 = _SetCameraEffects
        L1_3 = 0
        L2_3 = "SetDOF"
        L3_3 = 0
        L4_3 = 0
        L5_3 = L6_2.tDepthOfField[4] - 0.5
        L6_3 = L6_2.tDepthOfField[4] - 0.1
        L7_3 = L6_2.tDepthOfField[5] + 0.2
        L8_3 = L6_2.tDepthOfField[5] + 5
        L9_3 = L6_2.tDepthOfField[9]
        if not L9_3 then
          L9_3 = 1
        end
        L0_3(L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3)
      end
      L0_3 = type(L6_2.tFieldOfView)
      if L0_3 == "table" then
        _SetCameraEffects(0, "SetFOV", 0, L6_2.tFieldOfView[2])
      end
    end
    
    L6_2._CameraEffectsTimer = L7_2(L8_2, L9_2, L10_2)
  end
end

_ProcessCameraEffects = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2)
  local L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  if A1_2 == "RestoreAll" then
    Graphics.Camera.RestoreFovParams(A0_2, A2_2)
    Graphics.Camera.RestoreFocusParams(A0_2, A2_2)
  elseif A1_2 == "RestoreFOV" then
    Graphics.Camera.RestoreFovParams(A0_2, A2_2)
  elseif A1_2 == "RestoreDOF" then
    Graphics.Camera.RestoreFocusParams(A0_2, A2_2)
  elseif A1_2 == "SetDOF" then
    Graphics.Camera.SetFocusParams(A0_2, A4_2, A5_2, A6_2, A7_2, A8_2, A2_2)
  elseif A1_2 == "SetFOV" then
    Graphics.Camera.SetFovParams(A0_2, A3_2, A2_2)
  end
end

_SetCameraEffects = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  _SetCameraEffects(0, "SetDOF", 0, _tDefaultCameraEffects.DOF.nAngle, _tDefaultCameraEffects.DOF.nStartNear, _tDefaultCameraEffects.DOF.nEndNear, _tDefaultCameraEffects.DOF.nStartFar, _tDefaultCameraEffects.DOF.nEndFar, _tDefaultCameraEffects.DOF.nBlur)
  _SetCameraEffects(0, "SetFOV", 0, _tDefaultCameraEffects.FOV.nAngle)
end

_SetDefaultCameraEffects = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  L3_2 = _bFadedIn
  if L3_2 == A0_2 then
    MrxUtil.CallWithOptionalArgs(A1_2, A2_2)
    return
  end
  if A0_2 then
    MrxState.Exit(MrxState.STATE_WAITFORGAME, A1_2, A2_2)
  else
    MrxState.Enter(MrxState.STATE_WAITFORGAME, A1_2, A2_2)
  end
  _bFadedIn = A0_2
end

_Fade = L0_1
NETEVENT_ENABLEMARKERS = 0
NETEVENT_DISABLEMARKERS = 1
NETEVENT_DISPLAYMENU = 2
NETEVENT_HIDEMENU = 3

function L0_1(A0_2)
  local L1_2, L2_2
  L1_2 = NETEVENT_ENABLEMARKERS
  if A0_2 == L1_2 then
    Gui.EnablePlayerMarkers(true)
  else
    L1_2 = NETEVENT_DISABLEMARKERS
    if A0_2 == L1_2 then
      Gui.EnablePlayerMarkers(false)
    else
      L1_2 = NETEVENT_DISPLAYMENU
      if A0_2 == L1_2 then
        _DisplayClientMenu()
      else
        L1_2 = NETEVENT_HIDEMENU
        if A0_2 == L1_2 then
          _CleanupClientMenu()
        end
      end
    end
  end
end

NetEventCallback = L0_1
