inherit("MrxLaserGuidedBomb")
import("MrxSupportDesignatorLaser")
tVOCues = {
  "Misha-None-Freeplay-Support-02",
  "Misha-None-Freeplay-Support-12",
  "Misha-None-Freeplay-Support-20"
}
sBomb = "Bunker Buster Projectile"
uBomb = Pg.GetGuidByName("Bunker Buster Projectile")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorLaser:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetModuleName("MrxBunkerBuster")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.uDeliveryVehicle)
  oNewSupport.sBomb = self.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(self.sBomb)
  oNewSupport.tVOCues = self.tVOCues
  oNewSupport.BombExplodes = self.BombExplodes
  return oNewSupport
end

function BombExplodes(self)
  Debug.Printf("Bunker Buster Explosion")
  Debug.Printf("self.uBomb = " .. tostring(self.uSpawnedBomb))
  local nBombX, nBombY, nBombZ = Object.GetPosition(self.uSpawnedBomb)
  Pg.Spawn("Explosion (Bunker Buster Stage 1)", nBombX, nBombY, nBombZ)
  Object.Kill(self.uSpawnedBomb)
  Event.Create(Event.TimerRelative, {0.5}, GroundExplosion, {
    "20",
    "0.04",
    nBombX,
    nBombY,
    nBombZ
  })
  Event.Create(Event.TimerRelative, {1}, GroundExplosion, {
    "45",
    "0.04",
    nBombX,
    nBombY,
    nBombZ
  })
  Event.Create(Event.TimerRelative, {1.5}, GroundExplosion, {
    "65",
    "0.05",
    nBombX,
    nBombY,
    nBombZ
  })
  Event.Create(Event.TimerRelative, {2.75}, FinalExplosion, {
    self,
    nBombX,
    nBombY,
    nBombZ
  })
end

function FinalExplosion(self, nBombX, nBombY, nBombZ)
  Debug.Printf("MrxBunkerBuster:FinalExplosion ..." .. tostring(self.sBomb))
  local uBomb = self.uBomb
  local bunkerBomb = Pg.GetGuidByName("Bunker Buster Projectile")
  local nukeBomb = Pg.GetGuidByName("Nuclear Bunker Buster Projectile")
  Debug.Printf("MrxBunkerBuster:FinalExplosion - GuidBBB " .. tostring(bunkerBomb) .. ", GuidNuke " .. tostring(nukeBomb) .. ", GuidUBOMB " .. tostring(self.uBomb) .. ", localBomb " .. tostring(uBomb))
  if uBomb == Pg.GetGuidByName("Bunker Buster Projectile") then
    Pg.Spawn("Explosion (Bunker Buster Stage 2)", nBombX, nBombY, nBombZ)
    Event.Create(Event.TimerRelative, {2}, AfterShock, {
      self,
      50,
      nBombX,
      nBombY,
      nBombZ
    })
  elseif uBomb == Pg.GetGuidByName("Nuclear Bunker Buster Projectile") then
    Pg.Spawn("global_particle_airstrike_tactnuke", nBombX, nBombY, nBombZ)
    Event.Create(Event.TimerRelative, {1}, function()
      MrxUtil.SpawnObject("global_particle_exp_shockwave_ground_tactnuke", Pg.GetGuidByName("loc_shockwave"))
    end)
    Event.Create(Event.TimerRelative, {2}, AfterShock, {
      self,
      80,
      nBombX,
      nBombY,
      nBombZ
    })
    Event.Create(Event.TimerRelative, {2}, Event.Post, {
      "Nuked",
      {
        nBombX,
        nBombY,
        nBombZ
      }
    })
  end
end

function GroundExplosion(radius, density, nBombX, nBombY, nBombZ)
  Graphics.Effect.Terrain("global_particle_exp_shockwave_ground_bunkerbuster", radius, density, nBombX, 0, nBombZ)
end

function AfterShock(self, nRadius, nBombX, nBombY, nBombZ)
  nRadius = nRadius or 50
  Event.Post("Busted", {
    nBombX,
    nBombY,
    nBombZ
  })
  local tBuildings = Pg.FastCollectBuildings(nBombX, nBombY, nBombZ, nRadius)
  Debug.Printf("KFound " .. tostring(table.getn(tBuildings)) .. " buildings")
  for i, building in pairs(tBuildings) do
    local r = math.randf() * i
    if 5 < r then
      r = math.randf() * r
    end
    Event.Create(Event.TimerRelative, {r}, Demolish, {building})
  end
end

function Demolish(building)
  local x, y, z = Object.GetPosition(building)
  if x then
    Pg.Spawn("Explosion (Airstike Bomb Final Strike)", x, y, z)
  end
end
