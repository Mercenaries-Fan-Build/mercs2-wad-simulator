inherit("MrxSupport")
import("MrxSupportDesignatorBeacon")

function Create(oSelf, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, oSelf)
  oSelf.__index = oSelf
  local oDesignator = MrxSupportDesignatorBeacon:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetAATestLevel("advanced")
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxHARMStrike")
  return oNewSupport
end

function DesignationCallback(oSelf)
  local nTargetXOffset = 0
  local nTargetZOffset = 200
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 35, -1, oSelf.uOwner)
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(Player.GetCharacter(oSelf.uOwner))
  local nOffsetX = (nTargetZ - nSpawnZ) * -1
  local nOffsetZ = nTargetX - nSpawnX
  local nNearOffsetX, nNearOffsetY, nNearOffsetZ = Math.Normalize(nOffsetX, 0, nOffsetZ)
  nNearOffsetX = nNearOffsetX * nTargetXOffset
  nNearOffsetZ = nNearOffsetZ * nTargetXOffset
  local nNearerX = nSpawnX - nTargetX
  local nNearerY = 0
  local nNearerZ = nSpawnZ - nTargetZ
  nNearerX, nNearerY, nNearerZ = Math.Normalize(nNearerX, 0, nNearerZ)
  nNearerX = nNearerX * nTargetZOffset
  nNearerZ = nNearerZ * nTargetZOffset
  local uJet = Airstrike.Flyby("Support Vehicle (F117)", nSpawnX, nSpawnZ, nSpawnX + nNearOffsetX + (nTargetX - nSpawnX) + nNearerX, nSpawnZ + nNearOffsetZ + (nTargetZ - nSpawnZ) + nNearerZ, nTargetY + 35, 80, Strike, {oSelf})
  oSelf.uJet = uJet
  Sound.CueSound(0, "vo_allies_a_Yes01")
  Sound.CueSound(0, "veh_b52_flyby")
end

function Strike(oSelf)
  local uHero = Player.GetCharacter(oSelf.uOwner)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(uHero)
  local oFilter = ObjectFilter.Create()
  oFilter:SetFilter("AA (Medium)")
  oFilter:SetRelation(uHero, "<", 0)
  local tTargets = Pg.FastCollectGroundVehicles(nSpawnX, nSpawnY, nSpawnZ, 200, oFilter)
  local nCount = 0
  for i, uTarget in ipairs(tTargets) do
    if Vehicle.GetDriver(uTarget) then
      Event.Create(Event.TimerRelative, {
        0.4 * nCount
      }, LaunchMissile, {oSelf, uTarget})
      nCount = nCount + 1
    end
  end
end

function BombExplodes(uBomb)
  local x, y, z = Object.GetPos(uBomb)
  Debug.Printf("Direct Hit!")
end

function LaunchMissile(oSelf, uTarget)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uJet)
  local nPX, nPY, nPZ = Object.Get
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(uTarget)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 30
  local uBomb = Airstrike.SpawnTargettedOrdnance("Vehicle AT Missile", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nSpeedScale, nVectorZ * nSpeedScale, uTarget, "impact", 1, nil, BombExplodes, {uBomb})
end
