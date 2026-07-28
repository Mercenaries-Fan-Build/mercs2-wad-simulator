inherit("MrxTaskContract")
import("MrxTaskRace")
import("MrxTaskObjectiveDeliver")
import("MrxLayerManager")
import("MrxMusic")
import("MrxAchievements")

function LoadAssets(self)
  vLayersMain = "VZ_state_PirCon001"
  vLayersStage = "VZ_state_PirCon001_staging"
  local tLayersToAdd = {vLayersMain, vLayersStage}
  MrxLayerManager.Add(tLayersToAdd, GetJetskis, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  uPlayer = Player.GetAnyCharacter()
  local vLayersStage = "VZ_state_PirCon001_staging"
  tHurry = {
    "Fiona.Race.HurryUp01",
    "Fiona.Race.HurryUp02",
    "Fiona.Race.HurryUp03",
    "Fiona.Race.HurryUp04"
  }
  nJetskies = table.getn(tJetskies)
  self:JetskiRace()
end

function JetskiRace(self)
  Debug.Printf([[
----------------------------------------
----------------------------------------
JetskiRace]])
  local sMissionName = "PirCon001"
  local vLayers = "VZ_state_PirCon001"
  local vLayersStage = "VZ_state_PirCon001_staging"
  self.oRaceObjective = self:CreateChild({
    sRaceMission = "PirCon001",
    sName = "JetskiRace",
    sModuleName = "MrxTaskRace",
    vTgtInclude = tJetskies,
    tTimerParams = {
      nStartTime = 8 - (self:GetNumCompletions() or 0) * 2,
      nStep = 1
    },
    nAddTime = 8 - (self:GetNumCompletions() or 0),
    tCourseLocs = {
      sMissionName .. "_checkpoint000",
      sMissionName .. "_checkpoint001",
      sMissionName .. "_checkpoint002",
      sMissionName .. "_checkpoint003",
      sMissionName .. "_checkpoint005",
      sMissionName .. "_checkpoint020",
      sMissionName .. "_checkpoint025",
      sMissionName .. "_checkpoint030",
      sMissionName .. "_checkpoint040",
      sMissionName .. "_checkpoint045",
      sMissionName .. "_checkpoint050",
      sMissionName .. "_checkpoint060",
      sMissionName .. "_checkpoint065",
      sMissionName .. "_checkpoint070",
      sMissionName .. "_checkpoint080",
      sMissionName .. "_checkpoint090",
      sMissionName .. "_checkpoint100",
      sMissionName .. "_checkpoint110",
      sMissionName .. "_checkpoint120",
      sMissionName .. "_checkpoint130",
      sMissionName .. "_checkpoint140",
      sMissionName .. "_checkpoint150",
      sMissionName .. "_checkpoint160",
      sMissionName .. "_checkpoint170",
      sMissionName .. "_checkpoint190",
      sMissionName .. "_checkpoint200",
      sMissionName .. "_checkpoint210",
      sMissionName .. "_checkpoint220",
      sMissionName .. "_checkpoint240",
      sMissionName .. "_checkpoint290"
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-MinorContract-Pir01-01"
    },
    fOnComplete = function()
      MrxLayerManager.Remove(vLayersStage)
      local uWinner = self.oRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      local isMultiplayer = Player.GetCurrentPlayers()
      if isMultiplayer == 2 then
        local uWinner = self.oRaceObjective._uWinner
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_WHEELS_OF_STEEL", uWinner)
      end
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end
  })
  eCueMusic = self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    oJetski01,
    "d",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mu_mission_pircon001_01"
  })
  eDireStraight = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    Pg.GetGuidByName("PirCon001_ConvergeTrigger_Loc"),
    "<",
    25,
    false,
    false
  }, function(self)
    Debug.Printf("*@*@*@*@*@*@*@*@*@*@*@ PIR CON 001: Checking proximity!!! @*@*@*@*@*@*@*@*@*@*@*")
    ConvergeShips(self, "Salton_Seahorse_Blocking_01", "SS_Blocking_Path_01", 0.475)
    ConvergeShips(self, "Salton_Seahorse_Blocking_02", "SS_Blocking_Path_02", 0.65)
  end, {self})
end

function ConvergeShips(self, oShipName, oShipPath, nHaste)
  Debug.Printf("*@*@*@*@*@*@*@*@*@*@*@ PIR CON 001: LOOK OUT FOR THOSE BOATS!!! (" .. oShipName .. ") @*@*@*@*@*@*@*@*@*@*@*")
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName(oShipName)),
    Goal = "PathMove",
    Target = Pg.GetGuidByName(oShipPath),
    Mode = "Oneway",
    Reverse = false,
    Haste = nHaste,
    Priority = "hiPri"
  })
end

function VehicleCheck(self)
  nJetskies = nJetskies - 1
  local tPlayers = Player.GetAllPlayers()
  if nJetskies < #tPlayers then
    self.Cancel(self)
  end
end

function GetJetskis(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  local uSpawn01 = Pg.GetGuidByName("PirCon001_SpawnJetski01")
  local uSpawn02 = Pg.GetGuidByName("PirCon001_SpawnJetski02")
  local x1, y1, z1 = Object.GetPosition(uSpawn01)
  local x2, y2, z2 = Object.GetPosition(uSpawn02)
  local nYaw1 = Object.GetYaw(uSpawn01)
  local nYaw2 = Object.GetYaw(uSpawn02)
  local nPlayers = Player.GetCurrentPlayers()
  if nPlayers == 2 then
    oJetski01 = Pg.Spawn("Jetski (PR)", x1, y1, z1, nYaw1)
    oJetski02 = Pg.Spawn("Jetski (PR)", x2, y2, z2, nYaw2)
    tJetskies = {oJetski01, oJetski02}
  else
    oJetski01 = Pg.Spawn("Jetski (PR)", x1, y1, z1, nYaw1)
    tJetskies = {oJetski01}
  end
  eAssetsReady = Event.Create(Event.ObjectHibernation, {oJetski01, "awake"}, AssetsLoaded, {self})
end

function CrowdNoise(self)
  self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
    MrxVoSequence.Start({
      "PirThug-In-Mission-MinorContract-Pir01-02"
    })
    self:_CreateEvent(Event.TimerRelative, {1}, function(self)
      MrxVoSequence.Start({
        "PirThug-In-Mission-MinorContract-Pir01-03"
      })
      self:_CreateEvent(Event.TimerRelative, {1}, function(self)
        MrxVoSequence.Start({
          "PirThug-In-Mission-MinorContract-Pir01-04"
        })
        self:_CreateEvent(Event.TimerRelative, {1}, function(self)
          MrxVoSequence.Start({
            "PirThug-In-Mission-MinorContract-Pir01-05"
          })
        end, {self})
      end, {self})
    end, {self})
  end, {self})
end

function Cleanup(self)
  Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 PirCon001 is cleaning up now!")
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.MarkForRemoval(vLayersStage)
  for i, uVeh in ipairs(tJetskies) do
    Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 Setting up removal events for " .. tostring(uVeh))
    eRemoveJetski = Event.Create(Event.ObjectHibernation, {uVeh, "hibernated"}, Object.Remove, {uVeh})
  end
  MrxTaskContract.Cleanup(self)
end
