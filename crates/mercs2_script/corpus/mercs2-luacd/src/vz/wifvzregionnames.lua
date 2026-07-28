import("MrxPlayState")
import("MrxState")
import("MrxVoSequence")
import("DangerousBuilding")
tBoundaryList = {
  poi_AlliedHQ = {
    VO = "Fiona.POI.AlliedHQ01"
  },
  poi_Altagracia = {},
  poi_Amazonas = {
    VO = "Fiona.POI.WestAmazon01"
  },
  poi_AngelFalls = {
    VO = "Fiona.POI.AngelFalls01"
  },
  poi_Cambias = {},
  poi_cantina = {},
  poi_Caracas = {
    VO = "Fiona.POI.Caracas01"
  },
  poi_caracasbridge = {},
  poi_caracasdowntown = {},
  poi_caracashighway = {},
  poi_caracaspark = {},
  poi_caracasport = {},
  poi_ChinaHQ = {
    VO = "Fiona.POI.ChinaHQ01"
  },
  poi_Cumana = {
    VO = "Fiona.POI.Cumana02"
  },
  poi_CumanaFortress = {},
  poi_maracaibohotel = {},
  poi_CumanaWest = {},
  poi_CumanaMarket = {},
  poi_CumanaTheatre = {},
  poi_CumanaBridgeSouth = {},
  poi_CumanaBridgeNorth = {},
  poi_CumanaPark = {},
  poi_FortressIsland = {},
  poi_Guanare = {
    VO = "Fiona.POI.Guanare01"
  },
  poi_GuerillaHQ = {
    VO = "Fiona.POI.GurHQ01"
  },
  poi_IslaDeMano = {},
  poi_LakeMaracaibo = {
    VO = "Fiona.POI.LakeMaracaibo01"
  },
  poi_Maracaibo = {
    VO = "Fiona.POI.Maracaibo02"
  },
  poi_MaracaiboDocks = {
    VO = "Fiona.POI.MaracaiboRefinery01"
  },
  poi_MaracaiboPark = {},
  poi_maracaibobridge = {
    VO = "Fiona.POI.MaracaiboBridge01"
  },
  poi_maracaibohighway = {
    VO = "Fiona.POI.MaracaiboHighway01"
  },
  poi_maracaibowest = {
    VO = "Fiona.POI.WestMaracaibo01"
  },
  poi_Margarita = {
    VO = "Fiona.POI.Isla01"
  },
  poi_Merida = {
    VO = "Fiona.POI.Merida02"
  },
  poi_meridapark = {},
  poi_meridastadium = {},
  poi_OilDepot = {},
  poi_OilHQ = {
    VO = "Fiona.POI.UPHQ01"
  },
  poi_PirateHQ = {
    VO = "Fiona.POI.Pirates01"
  },
  poi_PMCHQ = {},
  poi_ShantyTown = {
    VO = "Fiona.POI.ShantyTown01"
  }
}
_tBoundaryEvents = {}

function Start()
  for sBoundaryName, uBoundaryEvent in pairs(_tBoundaryEvents) do
    Event.Delete(uBoundaryEvent)
    _tBoundaryEvents[sBoundaryName] = nil
  end
  if tBoundaryList then
    for sBoundaryName, tBoundaryData in pairs(tBoundaryList) do
      SetupBoundaryEvent(sBoundaryName, "enter")
    end
  end
  SetupDBBoundary()
end

function SetupDBBoundary()
  DangerousBuilding.SetRarity("all", "default")
  Debug.Printf("Re-enabling all DBs--you have exited Caracas (or you're not there)")
  Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("DisableDBs"),
    "enter",
    false
  }, DisableDBs)
end

function DisableDBs()
  DangerousBuilding.SetRarity("all", "never")
  Debug.Printf("Disabling all DBs--you have entered Caracas")
  Event.Create(Event.Boundary, {
    Player.GetAllCharacters(),
    Pg.GetGuidByName("DisableDBs"),
    "exit",
    false
  }, SetupDBBoundary)
end

function SetupBoundaryEvent(sBoundaryName, sAction)
  local uBoundaryGuid = Pg.GetGuidByName(sBoundaryName)
  if not uBoundaryGuid then
    return
  end
  if _tBoundaryEvents[sBoundaryName] then
    return
  end
  _tBoundaryEvents[sBoundaryName] = Event.Create(Event.Boundary, {
    Player.GetLocalCharacter(),
    uBoundaryGuid,
    sAction,
    false
  }, CrossedBoundary, {sBoundaryName})
end

function CrossedBoundary(sBoundaryName, uPlayerGuid, uBoundaryGuid, sAction)
  if not sBoundaryName then
    return
  end
  Event.Delete(_tBoundaryEvents[sBoundaryName])
  _tBoundaryEvents[sBoundaryName] = nil
  local sBoundaryLabel = Object.GetLocalizedName(uBoundaryGuid)
  if sAction == "enter" then
    if sBoundaryLabel then
      Hud.MapLabel:Show({sLocation = sBoundaryLabel, nDuration = 10})
    end
    local tBoundaryData = tBoundaryList[sBoundaryName]
    if not Net.IsClient() and type(tBoundaryData) == "table" and tBoundaryData.VO and not tBoundaryData.bVoPlayed and MrxPlayState.IsFree() and not MrxState.IsLocked() then
      tBoundaryData.bVoPlayed = true
      MrxVoSequence.Start(tBoundaryData.VO, nil, MrxVoSequence.knPriorityFreeplay)
    end
    SetupBoundaryEvent(sBoundaryName, "exit")
  elseif sAction == "exit" then
    SetupBoundaryEvent(sBoundaryName, "enter")
  end
end

function SaveSingleton()
  tSaveData = {}
  for sBoundaryName, tBoundaryData in pairs(tBoundaryList) do
    if tBoundaryData.bVoPlayed then
      table.insert(tSaveData, sBoundaryName)
    end
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  if not type(tSaveData) == "table" then
    return
  end
  for i, sBoundaryName in ipairs(tSaveData) do
    local tBoundaryData = tBoundaryList[sBoundaryName]
    if type(tBoundaryData) == "table" then
      tBoundaryData.bVoPlayed = true
    end
  end
end
